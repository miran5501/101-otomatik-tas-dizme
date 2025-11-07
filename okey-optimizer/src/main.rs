use actix_web::{post, web, App, HttpResponse, HttpServer, Responder, middleware::Logger};
use actix_web::web::JsonConfig;
use actix_cors::Cors;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============ RENK SIRALAMA ============

fn color_order(renk: &str) -> u8 {
    match renk {
        "kırmızı" | "kirmizi" => 0,
        "siyah" => 1,
        "mavi" => 2,
        "turuncu" => 3,
        _ => 99,
    }
}

// ============ DOMAIN ============

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
struct Tile {
    renk: String,
    sayi: u8,
    #[serde(default)]
    okey: bool,
    #[serde(default)]
    sahteOkey: bool,
}

#[derive(Clone, Debug)]
struct UseTile {
    renk: Option<String>,
    sayi: u8,
    is_okey: bool,
}

#[derive(Clone, Debug)]
struct Candidate {
    is_serie: bool,
    serie_renk: Option<String>,
    group_sayi: Option<u8>,
    uses: Vec<UseTile>,
    score: i32,
}

// ============ HAND STATE ============

#[derive(Clone)]
struct HandState {
    counts: HashMap<(String, u8), usize>,
    jokers: usize,
}

impl HandState {
    fn from_tiles(tiles: &[Tile]) -> Self {
        let mut counts = HashMap::new();
        let mut jokers = 0;
        for t in tiles {
            if t.okey {
                jokers += 1;
            } else {
                *counts.entry((t.renk.to_lowercase(), t.sayi)).or_insert(0) += 1;
            }
        }
        Self { counts, jokers }
    }

    fn can_apply(&self, c: &Candidate) -> bool {
        let mut needed: HashMap<(String, u8), usize> = HashMap::new();
        let mut need_jokers = 0;
        for u in &c.uses {
            if u.is_okey {
                need_jokers += 1;
            } else if let Some(renk) = &u.renk {
                *needed.entry((renk.to_lowercase(), u.sayi)).or_insert(0) += 1;
            }
        }
        if need_jokers > self.jokers {
            return false;
        }
        for (k, n) in needed {
            if self.counts.get(&k).copied().unwrap_or(0) < n {
                return false;
            }
        }
        true
    }

    fn apply(&mut self, c: &Candidate) {
        for u in &c.uses {
            if u.is_okey {
                self.jokers -= 1;
            } else if let Some(renk) = &u.renk {
                let key = (renk.to_lowercase(), u.sayi);
                if let Some(cnt) = self.counts.get_mut(&key) {
                    *cnt -= 1;
                }
            }
        }
    }

    fn unapply(&mut self, c: &Candidate) {
        for u in &c.uses {
            if u.is_okey {
                self.jokers += 1;
            } else if let Some(renk) = &u.renk {
                *self.counts.entry((renk.to_lowercase(), u.sayi)).or_insert(0) += 1;
            }
        }
    }

    fn to_tiles(&self) -> Vec<Tile> {
        let mut v = Vec::new();
        let mut sorted_keys: Vec<_> = self.counts.iter().collect();
        sorted_keys.sort_by_key(|((renk, sayi), _)| (color_order(renk), *sayi));
        for ((renk, sayi), cnt) in sorted_keys {
            for _ in 0..*cnt {
                v.push(Tile {
                    renk: renk.clone(),
                    sayi: *sayi,
                    okey: false,
                    sahteOkey: false,
                });
            }
        }
        for _ in 0..self.jokers {
            v.push(Tile {
                renk: "okey".into(),
                sayi: 0,
                okey: true,
                sahteOkey: false,
            });
        }
        v
    }
}

// ============ CANDIDATES ============

fn generate_candidates(hand: &HandState) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    generate_groups(hand, &mut candidates);
    generate_series(hand, &mut candidates);
    candidates.sort_by_key(|c| -c.score);
    candidates
}

fn generate_groups(hand: &HandState, candidates: &mut Vec<Candidate>) {
    let mut by_number: HashMap<u8, Vec<String>> = HashMap::new();
    for ((renk, sayi), cnt) in &hand.counts {
        if *cnt > 0 {
            by_number.entry(*sayi).or_default().push(renk.clone());
        }
    }
    for (sayi, colors) in by_number {
        let mut unique_colors: Vec<String> = colors.into_iter().unique().collect();
        unique_colors.sort_by_key(|c| color_order(c));
        for group_size in 3..=4 {
            let max_real = unique_colors.len().min(group_size);
            for real_count in 0..=max_real {
                let joker_count = group_size - real_count;
                if joker_count > hand.jokers {
                    continue;
                }
                for subset in unique_colors.iter().combinations(real_count) {
                    let mut uses = Vec::with_capacity(group_size);
                    let mut sorted_subset = subset.clone();
                    sorted_subset.sort_by_key(|c| color_order(c));
                    for renk in sorted_subset {
                        uses.push(UseTile {
                            renk: Some(renk.clone()),
                            sayi,
                            is_okey: false,
                        });
                    }
                    for _ in 0..joker_count {
                        uses.push(UseTile {
                            renk: None,
                            sayi,
                            is_okey: true,
                        });
                    }
                    let score = (group_size as i32) * (sayi as i32);
                    candidates.push(Candidate {
                        is_serie: false,
                        serie_renk: None,
                        group_sayi: Some(sayi),
                        uses,
                        score,
                    });
                }
            }
        }
    }
}

fn generate_series(hand: &HandState, candidates: &mut Vec<Candidate>) {
    let mut by_color: HashMap<String, Vec<u8>> = HashMap::new();
    for ((renk, sayi), cnt) in &hand.counts {
        if *cnt > 0 {
            by_color.entry(renk.clone()).or_default().push(*sayi);
        }
    }
    for (renk, mut numbers) in by_color {
        numbers.sort_unstable();
        let available: Vec<u8> = numbers.into_iter().unique().collect();
        for serie_len in 3..=5 {
            for start in 1..=13 {
                let end = start + serie_len - 1;
                if end > 13 {
                    break;
                }
                let mut missing = 0;
                let mut uses = Vec::with_capacity(serie_len as usize);
                let mut total_score = 0;
                for num in start..=end {
                    let num_u8 = num as u8;
                    if available.contains(&num_u8) {
                        uses.push(UseTile {
                            renk: Some(renk.clone()),
                            sayi: num_u8,
                            is_okey: false,
                        });
                    } else {
                        missing += 1;
                        uses.push(UseTile {
                            renk: Some(renk.clone()),
                            sayi: num_u8,
                            is_okey: true,
                        });
                    }
                    total_score += num;
                }
                if missing <= hand.jokers {
                    candidates.push(Candidate {
                        is_serie: true,
                        serie_renk: Some(renk.clone()),
                        group_sayi: None,
                        uses,
                        score: total_score,
                    });
                }
            }
        }
    }
}

// ============ BACKTRACKING ============

fn search_best(
    idx: usize,
    candidates: &[Candidate],
    state: &mut HandState,
    cur_score: i32,
    cur_sel: &mut Vec<usize>,
    best: &mut (i32, Vec<usize>),
) {
    if idx == candidates.len() {
        if cur_score > best.0 {
            best.0 = cur_score;
            best.1 = cur_sel.clone();
        }
        return;
    }
    search_best(idx + 1, candidates, state, cur_score, cur_sel, best);
    let cand = &candidates[idx];
    if state.can_apply(cand) {
        state.apply(cand);
        cur_sel.push(idx);
        search_best(idx + 1, candidates, state, cur_score + cand.score, cur_sel, best);
        cur_sel.pop();
        state.unapply(cand);
    }
}

// ============ INDEX & API ============

#[derive(Serialize)]
struct IndexedTile {
    index: i32,
    renk: String,
    sayi: u8,
    okey: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    sahteOkey: Option<bool>,
}

#[derive(Serialize)]
struct IndexedMeld {
    tip: String,
    taslar: Vec<IndexedTile>,
    puan: i32,
}

#[derive(Serialize)]
struct ApiResponse {
    toplam_puan: i32,
    dizilimler: Vec<IndexedMeld>,
    elde_kalanlar: Vec<IndexedTile>,
}

fn assign_indexes(
    selected: &[&Candidate],
    leftovers: Vec<Tile>,
) -> (Vec<IndexedMeld>, Vec<IndexedTile>) {
    let mut current_index = 1;
    let mut indexed_melds = Vec::new();
    for cand in selected {
        let len = cand.uses.len() as i32;
        if current_index <= 18 && current_index + len - 1 > 18 {
            current_index = 19;
        }
        let tiles: Vec<IndexedTile> = cand
            .uses
            .iter()
            .enumerate()
            .map(|(i, u)| IndexedTile {
                index: current_index + i as i32,
                renk: u.renk.clone().unwrap_or_else(|| "okey".into()),
                sayi: u.sayi,
                okey: u.is_okey,
                sahteOkey: None,
            })
            .collect();
        let tip = if cand.is_serie { "seri" } else { "grup" }.to_string();
        indexed_melds.push(IndexedMeld { tip, taslar: tiles, puan: cand.score });
        current_index += len + 1;
    }
    let mut current_left = 36;
    let mut indexed_leftovers = Vec::new();
    for t in leftovers {
        indexed_leftovers.push(IndexedTile {
            index: current_left,
            renk: t.renk,
            sayi: t.sayi,
            okey: t.okey,
            sahteOkey: if t.sahteOkey { Some(true) } else { None },
        });
        current_left -= 1;
    }
    (indexed_melds, indexed_leftovers)
}

#[post("/dizilim-al")]
async fn optimize(payload: web::Json<Vec<Tile>>) -> impl Responder {
    let tiles = payload.into_inner();
    let original_state = HandState::from_tiles(&tiles);
    let candidates = generate_candidates(&original_state);
    let mut state = original_state.clone();
    let mut best: (i32, Vec<usize>) = (0, Vec::new());
    let mut cur_sel = Vec::new();
    search_best(0, &candidates, &mut state, 0, &mut cur_sel, &mut best);
    let mut final_state = original_state.clone();
    let selected_refs: Vec<&Candidate> = best.1.iter().map(|&i| &candidates[i]).collect();
    for c in &selected_refs {
        final_state.apply(c);
    }
    let (indexed_melds, indexed_leftovers) = assign_indexes(&selected_refs, final_state.to_tiles());
    HttpResponse::Ok().json(ApiResponse {
        toplam_puan: best.0,
        dizilimler: indexed_melds,
        elde_kalanlar: indexed_leftovers,
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
    std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10000)
}
