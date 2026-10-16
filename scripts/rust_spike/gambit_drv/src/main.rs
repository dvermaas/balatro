// Lockstep driver for Gambit's balatro-core: one command per stdin line, one JSON state per stdout line.
use balatro_core::cards::Card;
use balatro_core::run::{BlindStage, Run, State};
use balatro_core::shop::ShopItemKind;
use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn q(s: &str) -> String { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }
fn arr(v: Vec<String>) -> String { format!("[{}]", v.join(",")) }
fn rank(c: &Card) -> &'static str {
    ["", "", "2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A"][c.rank.id() as usize]
}
fn cs(c: &Card) -> String { format!("{}{}", rank(c), c.suit.key()) }

fn dump(r: &Run, err: &str, total: f64) -> String {
    let phase = match r.state() { State::BlindSelect => "BLIND", State::SelectingHand => "PLAY", State::RoundEval => "EVAL",
        State::Shop => "SHOP", State::PackOpen => "PACK", State::GameOver => "OVER", State::Won => "WON", _ => "?" };
    let blind = match r.blind_on_deck() { BlindStage::Small => 0, BlindStage::Big => 1, BlindStage::Boss => 2 };
    let hand = arr(r.hand().iter().map(|c| q(&cs(c))).collect());
    let jokers = arr(r.jokers().iter().map(|j| q(j.id.key())).collect());
    let (mut slots, mut vouchers, mut packs) = (vec![], vec![], vec![]);
    if let Some(s) = r.shop() {
        for it in &s.jokers {
            slots.push(q(&match &it.kind { ShopItemKind::Joker(j) => j.id.key().to_string(),
                ShopItemKind::Consumable(c) => c.key.to_string(), ShopItemKind::PlayingCard(c) => cs(c),
                #[allow(unreachable_patterns)] _ => "?".to_string() }));
        }
        for v in &s.vouchers { vouchers.push(q(v.key)); }
        for p in &s.packs { if !p.used { packs.push(q(p.key)); } }
    }
    let (ts, tb) = r.blind_tags();
    let pack = arr(r.pack().map(|p| p.items.iter().map(|it| q(&match it {
        balatro_core::shop::PackItem::Joker(j) => j.id.key().to_string(),
        balatro_core::shop::PackItem::Consumable(c) => c.key.to_string(),
        balatro_core::shop::PackItem::PlayingCard(c) => cs(c) })).collect()).unwrap_or_default());
    format!("{{\"phase\":{},\"ante\":{},\"blind\":{},\"boss\":{},\"money\":{},\"total\":{},\"deck_n\":{},\"hand\":{},\"jokers\":{},\"slots\":{},\"vouchers\":{},\"packs\":{},\"tags\":[{},{}],\"pack\":{},\"err\":{}}}",
        q(phase), r.ante(), blind, q(r.boss_choice()), r.dollars(), total, r.deck_len(), hand, jokers,
        arr(slots), arr(vouchers), arr(packs), q(ts), q(tb), pack, q(err))
}

fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout();
    let mut run: Option<Run> = None;
    let mut total = 0.0f64; // summed hand scores, like rumbot's chips_scored
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let mut it = line.splitn(2, ' ');
        let cmd = it.next().unwrap_or("");
        let arg = it.next().unwrap_or("");
        if cmd == "new" { run = Some(Run::new(arg)); total = 0.0; writeln!(out, "{}", dump(run.as_ref().unwrap(), "", total)).unwrap(); out.flush().unwrap(); continue; }
        let r = run.as_mut().unwrap();
        let before = r.chips();
        let res = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
            match cmd {
                "select" => r.select_blind().map_err(|e| format!("{:?}", e)),
                "play" => { let idx: Vec<usize> = arg.split(',').map(|x| x.parse().unwrap()).collect();
                            r.play(&idx).map(|_| ()).map_err(|e| format!("{:?}", e)) }
                "discard" => { let idx: Vec<usize> = arg.split(',').map(|x| x.parse().unwrap()).collect();
                            r.discard(&idx).map_err(|e| format!("{:?}", e)) }
                "cash" => r.cash_out().map_err(|e| format!("{:?}", e)),
                "buy" => r.buy_shop_item(arg.parse().unwrap()).map_err(|e| format!("{:?}", e)),
                "skip" => r.skip_blind().map_err(|e| format!("{:?}", e)),
                "voucher" => r.redeem_voucher(arg.parse().unwrap()).map_err(|e| format!("{:?}", e)),
                "pack" => r.buy_pack(arg.parse().unwrap()).map_err(|e| format!("{:?}", e)),
                "pick" => r.pick_pack_item(arg.parse().unwrap(), &[]).map_err(|e| format!("{:?}", e)),
                "leave" => r.leave_shop().map_err(|e| format!("{:?}", e)),
                "skippack" => r.skip_pack().map_err(|e| format!("{:?}", e)),
                _ => Err("bad command".into()),
            }
        }));
        let err = match res { Ok(Ok(())) => String::new(), Ok(Err(e)) => e, Err(_) => "panic".into() };
        if cmd == "play" && err.is_empty() {
            // chips() is the round total; after a winning hand the round may already be reset
            let after = r.chips();
            total += if after >= before { after - before } else { f64::NAN };
        }
        writeln!(out, "{}", dump(r, &err, total)).unwrap();
        out.flush().unwrap();
    }
}
