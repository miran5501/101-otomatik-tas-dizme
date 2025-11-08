use actix_web::{post, web, App, HttpResponse, HttpServer, Responder, middleware::Logger};
use actix_web::web::JsonConfig;
use actix_cors::Cors;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;


/// =========================
/// ======== DOMAIN =========
/// =========================

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
pub struct Tile {
    pub renk: String,  // "kirmizi","mavi","turuncu","siyah"
    pub sayi: u8,      // 1..13
    #[serde(default)]
    pub okey: bool,    // gerçek okey
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum Color { Kirmizi, Mavi, Turuncu, Siyah }

fn color_from(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "kirmizi" | "kırmızı" => Some(Color::Kirmizi),
        "mavi" => Some(Color::Mavi),
        "turuncu" => Some(Color::Turuncu),
        "siyah" => Some(Color::Siyah),
        _ => None,
    }
}
fn color_to_str(c: Color) -> &'static str {
    match c {
        Color::Kirmizi => "kirmizi",
        Color::Mavi => "mavi",
        Color::Turuncu => "turuncu",
        Color::Siyah => "siyah",
    }
}

#[derive(Clone, Debug)]
struct TileInt {
    color: Color,
    number: u8,
    is_okey: bool,
    idx: usize,
}

#[derive(Clone, Debug)]
struct Group {
    mask: u128,
    score: i32,
    kind: GroupKind,
    tiles: Vec<(Color, u8, bool)>,
}

#[derive(Clone, Debug)]
enum GroupKind { Run, Set }

#[derive(Clone, Debug)]
struct BestSolution {
    total_score: i32,
    groups: Vec<Group>,
}


/// =========================
/// ========= ENTRY =========
/// =========================

#[post("/dizilim-al")]
async fn optimize(payload: web::Json<Vec<Tile>>) -> impl Responder {
    let tiles_in = payload.into_inner();
    let result = best_layout(tiles_in.clone());
    let (dizilimler, elde_kalanlar) = assign_indexes(&result, &tiles_in);
    HttpResponse::Ok().json(ApiResponse {
        toplam_puan: result.total_score,
        dizilimler,
        elde_kalanlar,
    })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let json_cfg = JsonConfig::default().limit(64 * 1024);
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["POST"])
            .allowed_headers(vec!["Content-Type"]);
        App::new()
            .wrap(Logger::default())
            .wrap(cors)
            .app_data(json_cfg.clone())
            .service(optimize)
    })
    .bind(("0.0.0.0", get_port()))?
    .run()
    .await
}

fn get_port() -> u16 {
    std::env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(10000)
}


/// =========================
/// ========= LOGIC =========
/// =========================

fn best_layout(tiles_in: Vec<Tile>) -> BestSolution {
    let mut tiles: Vec<TileInt> = Vec::with_capacity(tiles_in.len());
    for (i, t) in tiles_in.iter().enumerate() {
        if let Some(c) = color_from(&t.renk) {
            tiles.push(TileInt {
                color: c,
                number: t.sayi.min(13).max(1),
                is_okey: t.okey,
                idx: i,
            });
        } else if t.okey {
            tiles.push(TileInt {
                color: Color::Kirmizi,
                number: 0,
                is_okey: true,
                idx: i,
            });
        }
    }

    if tiles.len() == 22 {
        let mut best: Option<(i32, Vec<Group>)> = None;
        for drop_idx in 0..22 {
            let mut filtered = tiles.clone();
            filtered.remove(drop_idx);
            let (score, groups, _) = solve_21(&filtered);
            if best.is_none() || score > best.as_ref().unwrap().0 {
                best = Some((score, groups));
            }
        }
        let (score, groups) = best.unwrap();
        BestSolution { total_score: score, groups }
    } else {
        let (score, groups, _) = solve_21(&tiles);
        BestSolution { total_score: score, groups }
    }
}

fn solve_21(tiles: &Vec<TileInt>) -> (i32, Vec<Group>, Vec<usize>) {
    let mut okeys = Vec::new();
    let mut map_cn: HashMap<(Color,u8), Vec<usize>> = HashMap::new();
    let mut map_n_c: HashMap<u8, HashMap<Color, Vec<usize>>> = HashMap::new();

    for t in tiles {
        if t.is_okey {
            okeys.push(t.idx);
        } else {
            map_cn.entry((t.color,t.number)).or_default().push(t.idx);
            map_n_c.entry(t.number).or_default().entry(t.color).or_default().push(t.idx);
        }
    }

    let mut groups = Vec::new();
    generate_sets(&mut groups, &map_n_c, &okeys);
    generate_runs(&mut groups, &map_cn, &okeys);

    groups.sort_by(|a,b| b.score.cmp(&a.score));
    let (best_score, take_ids) = select_best(&groups);
    let mut used_mask = 0u128;
    let mut chosen = Vec::new();
    for &i in &take_ids {
        used_mask |= groups[i].mask;
        chosen.push(groups[i].clone());
    }

    let mut leftover = Vec::new();
    for t in tiles {
        if (used_mask & (1u128 << t.idx)) == 0 {
            leftover.push(t.idx);
        }
    }
    (best_score, chosen, leftover)
}

fn generate_sets(out: &mut Vec<Group>,
    map: &HashMap<u8, HashMap<Color, Vec<usize>>>,
    okeys: &Vec<usize>) {
    use Color::*;
    let colors = [Kirmizi,Mavi,Turuncu,Siyah];

    for n in 1u8..=13 {
        let mut have = Vec::new();
        if let Some(cmap) = map.get(&n) {
            for &c in &colors {
                if let Some(v) = cmap.get(&c) {
                    if !v.is_empty(){ have.push((c,v.clone())); }
                }
            }
        }

        for size in [3,4] {
            for k in 1..=have.len().min(size) {
                let jok_need = size - k;
                if jok_need > okeys.len() { continue; }

                for combo in combinations_of(&have,k) {
                    let per: Vec<&Vec<usize>> = combo.iter().map(|(_,v)| v).collect();
                    for real_pick in choose_one_from_each(&per) {
                        for jok_pick in choose_k(okeys, jok_need) {
                            let mut mask = 0;
                            let mut tiles_data = Vec::new();

                            for &ri in &real_pick {
                                mask |= 1u128 << ri;
                                let color = combo.iter().find(|(_,v)| v.contains(&ri)).map(|(c,_)| *c).unwrap_or(Kirmizi);
                                tiles_data.push((color, n, false));
                            }
                            for &ji in &jok_pick {
                                mask |= 1u128 << ji;
                                tiles_data.push((Kirmizi, n, true));
                            }

                            out.push(Group { mask, score: n as i32 * size as i32, kind: GroupKind::Set, tiles: tiles_data });
                        }
                    }
                }
            }
        }
    }
}

fn generate_runs(out: &mut Vec<Group>,
    map: &HashMap<(Color,u8),Vec<usize>>,
    okeys: &Vec<usize>) {
    use Color::*;
    for &c in &[Kirmizi,Mavi,Turuncu,Siyah] {
        for len in [3u8,4,5] {
            for start in 1..=13 {
                let end = start + len - 1;
                if end > 13 { break; }

                let mut slots = Vec::with_capacity(len as usize);
                let mut missing_positions = Vec::new();
                for (offset, n) in (start..=end).enumerate() {
                    if let Some(v) = map.get(&(c, n)) {
                        slots.push(v.clone());
                    } else {
                        slots.push(Vec::new());
                        missing_positions.push(offset);
                    }
                }
                let miss = missing_positions.len();
                if miss > okeys.len() { continue; }

                let real_positions: Vec<usize> = (0..len as usize).filter(|&i| !slots[i].is_empty()).collect();
                let per: Vec<&Vec<usize>> = real_positions.iter().map(|&p| &slots[p]).collect();

                for real_pick in choose_one_from_each(&per) {
                    let mut chosen_map = HashMap::new();
                    for (i_pos, &idx_val) in real_positions.iter().zip(real_pick.iter()) {
                        chosen_map.insert(*i_pos, idx_val);
                    }

                    for jok_pick in choose_k(okeys, miss) {
                        let mut mask = 0;
                        let mut tiles_data = Vec::new();
                        let mut joker_it = jok_pick.iter();

                        for pos in 0..len as usize {
                            let num = start + pos as u8;
                            if let Some(&real_idx) = chosen_map.get(&pos) {
                                mask |= 1u128 << real_idx;
                                tiles_data.push((c, num, false));
                            } else {
                                let &joker_idx = joker_it.next().unwrap();
                                mask |= 1u128 << joker_idx;
                                tiles_data.push((c, num, true));
                            }
                        }

                        out.push(Group { mask, score: ((start + end) * len / 2) as i32, kind: GroupKind::Run, tiles: tiles_data });
                    }
                }
            }
        }
    }
}

fn select_best(groups: &Vec<Group>) -> (i32, Vec<usize>) {
    let mut prefix = vec![0; groups.len()+1];
    for i in (0..groups.len()).rev() { prefix[i] = prefix[i+1] + groups[i].score.max(0); }
    let mut best = 0; let mut best_t = vec![]; let mut cur = vec![];

    fn dfs(i: usize, used: u128, score: i32, groups: &Vec<Group>, prefix: &Vec<i32>, best: &mut i32, best_t: &mut Vec<usize>, cur: &mut Vec<usize>) {
        if i == groups.len() {
            if score > *best { *best = score; *best_t = cur.clone(); }
            return;
        }
        if score + prefix[i] <= *best { return; }
        if groups[i].mask & used == 0 {
            cur.push(i);
            dfs(i+1, used | groups[i].mask, score + groups[i].score, groups, prefix, best, best_t, cur);
            cur.pop();
        }
        dfs(i+1, used, score, groups, prefix, best, best_t, cur);
    }

    dfs(0, 0, 0, groups, &prefix, &mut best, &mut best_t, &mut cur);
    (best, best_t)
}

/// =============== HELPERS ===============

fn choose_k<T:Clone>(arr:&Vec<T>,k:usize)->Vec<Vec<T>>{
    let mut r=vec![]; if k==0{r.push(vec![]);return r;} if k>arr.len(){return r;}
    fn rec<T:Clone>(a:&Vec<T>,s:usize,k:usize,c:&mut Vec<T>,o:&mut Vec<Vec<T>>){
        if k==0{o.push(c.clone());return;}
        for i in s..=a.len()-k{c.push(a[i].clone());rec(a,i+1,k-1,c,o);c.pop();}
    } rec(arr,0,k,&mut vec![],&mut r); r
}
fn combinations_of<T:Clone>(arr:&Vec<T>,k:usize)->Vec<Vec<T>>{
    let mut r=vec![]; if k==0{r.push(vec![]);return r;} if k>arr.len(){return r;}
    fn rec<T:Clone>(a:&Vec<T>,s:usize,k:usize,c:&mut Vec<T>,o:&mut Vec<Vec<T>>){
        if k==0{o.push(c.clone());return;}
        for i in s..=a.len()-k{c.push(a[i].clone());rec(a,i+1,k-1,c,o);c.pop();}
    } rec(arr,0,k,&mut vec![],&mut r); r
}
fn choose_one_from_each<T:Clone>(lists:&Vec<&Vec<T>>)->Vec<Vec<T>>{
    if lists.is_empty(){return vec![vec![]];}
    let mut r=vec![];
    fn rec<T:Clone>(l:&Vec<&Vec<T>>,i:usize,c:&mut Vec<T>,o:&mut Vec<Vec<T>>){
        if i==l.len(){o.push(c.clone());return;}
        for x in l[i].iter(){c.push(x.clone());rec(l,i+1,c,o);c.pop();}
    } rec(lists,0,&mut vec![],&mut r); r
}


/// =========================
/// ========= OUTPUT =========
/// =========================

#[derive(Serialize)]
struct IndexedTile {
    index: i32,
    renk: String,
    sayi: u8,
    okey: bool,
}
#[derive(Serialize)]
struct IndexedMeld {
    tip:String,
    taslar:Vec<IndexedTile>,
    puan:i32,
}
#[derive(Serialize)]
struct ApiResponse {
    toplam_puan:i32,
    dizilimler:Vec<IndexedMeld>,
    elde_kalanlar:Vec<IndexedTile>,
}

fn assign_indexes(result:&BestSolution, original:&[Tile])->(Vec<IndexedMeld>,Vec<IndexedTile>){
    let mut current_index = 1;
    let mut melds = Vec::new();

    for g in &result.groups {
        let tip = match g.kind { GroupKind::Run => "seri", GroupKind::Set => "grup" }.to_string();
        let len = g.tiles.len() as i32;
        if current_index <= 18 && current_index + len - 1 > 18 { current_index = 19; }

        let tiles: Vec<IndexedTile> = g.tiles.iter().enumerate().map(|(i,(c,n,is_okey))| {
            IndexedTile{
                index: current_index + i as i32,
                renk: if *is_okey { "okey".into() } else { color_to_str(*c).into() },
                sayi: *n,
                okey: *is_okey,
            }
        }).collect();

        melds.push(IndexedMeld { tip, taslar: tiles, puan: g.score });
        current_index += len + 1;
    }

    let mut left_index = 36;
    let used_mask: u128 = result.groups.iter().fold(0u128, |acc, g| acc | g.mask);
    let leftovers: Vec<IndexedTile> = original.iter().enumerate()
        .filter(|(i, _)| (used_mask & (1u128 << i)) == 0)
        .map(|(_i, t)| IndexedTile {
            index: { left_index -= 1; left_index + 1 },
            renk: if t.okey { "okey".into() } else { t.renk.to_lowercase() },
            sayi: t.sayi,
            okey: t.okey,
        }).collect();

    (melds, leftovers)
}
