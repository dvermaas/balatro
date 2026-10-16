// Lockstep driver for rumbot-sim: one command per stdin line, one JSON state per stdout line.
use rumbot_sim::game::{Action, ActionType, GameState, Phase};
use std::io::{self, BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn q(s: &str) -> String { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }
fn arr(v: Vec<String>) -> String { format!("[{}]", v.join(",")) }

fn dump(g: &GameState, err: &str) -> String {
    let phase = match g.phase { Phase::BlindSelect => "BLIND", Phase::Playing => "PLAY", Phase::RoundEval => "EVAL",
        Phase::Shop => "SHOP", Phase::Pack => "PACK", Phase::GameOver => "OVER", Phase::Won => "WON" };
    let hand = arr(g.hand.iter().map(|c| { let c = c.borrow(); q(&format!("{}{}", c.rank.short(), c.suit.as_str())) }).collect());
    let jokers = arr(g.jokers.iter().map(|j| q(j.borrow().spec.name)).collect());
    let (mut slots, mut vouchers, mut packs) = (vec![], vec![], vec![]);
    if let Some(s) = &g.shop {
        for sl in &s.slots {
            let name = if let Some(j) = &sl.joker { j.borrow().spec.name.to_string() }
                else if let Some(c) = sl.consumable { c.name.to_string() }
                else if let Some(c) = &sl.card { let c = c.borrow(); format!("{}{}", c.rank.short(), c.suit.as_str()) }
                else { String::new() };
            slots.push(q(&name));
        }
        for v in &s.vouchers { vouchers.push(q(v.name)); }
        for p in &s.packs { packs.push(q(&format!("{:?}_{}", p.kind, p.size).to_lowercase())); }
    }
    let pack = arr(g.pack_options.iter().map(|o| q(&match o {
        rumbot_sim::game::PackChoice::Joker(j) => j.borrow().spec.name.to_string(),
        rumbot_sim::game::PackChoice::Card(c) => { let c = c.borrow(); format!("{}{}", c.rank.short(), c.suit.as_str()) }
        rumbot_sim::game::PackChoice::Consumable(c) => c.name.to_string() })).collect());
    let tags = arr(g.ante_tag_keys.iter().map(|t| q(t)).collect());
    format!("{{\"phase\":{},\"ante\":{},\"blind\":{},\"boss\":{},\"money\":{},\"total\":{},\"deck_n\":{},\"hand\":{},\"jokers\":{},\"slots\":{},\"vouchers\":{},\"packs\":{},\"tags\":{},\"pack\":{},\"err\":{},\"pending\":{},\"hands_left\":{},\"discards_left\":{}}}",
        q(phase), g.ante, g.blind_index, q(&g.ante_boss), g.money, g.chips_scored, g.draw_pile.len(), hand, jokers,
        arr(slots), arr(vouchers), arr(packs), tags, pack, q(err), g.pending_payout, g.hands_left, g.discards_left)
}

fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout();
    let mut g: Option<GameState> = None;
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let mut it = line.splitn(2, ' ');
        let cmd = it.next().unwrap_or("");
        let arg = it.next().unwrap_or("");
        if cmd == "new" { g = Some(GameState::new(arg, "Red Deck", 1)); writeln!(out, "{}", dump(g.as_ref().unwrap(), "")).unwrap(); out.flush().unwrap(); continue; }
        let gs = g.as_mut().unwrap();
        let action = match cmd {
            "select" => Action::new(ActionType::SelectBlind),
            "play" => Action::with_cards(ActionType::Play, arg.split(',').map(|x| x.parse().unwrap()).collect()),
            "discard" => Action::with_cards(ActionType::Discard, arg.split(',').map(|x| x.parse().unwrap()).collect()),
            "cash" => Action::new(ActionType::CashOut),
            "buy" => Action::at(ActionType::Buy, arg.parse().unwrap()),
            "leave" => Action::new(ActionType::LeaveShop),
            "skip" => Action::new(ActionType::SkipBlind),
            "voucher" => Action::at(ActionType::BuyVoucher, arg.parse().unwrap()),
            "pack" => Action::at(ActionType::BuyPack, arg.parse().unwrap()),
            "pick" => Action::at(ActionType::PickPack, arg.parse().unwrap()),
            "skippack" => Action::new(ActionType::SkipPack),
            _ => { writeln!(out, "{}", dump(gs, "bad command")).unwrap(); continue; }
        };
        if !gs.is_legal(&action) { writeln!(out, "{}", dump(gs, "illegal")).unwrap(); out.flush().unwrap(); continue; }
        let r = catch_unwind(AssertUnwindSafe(|| gs.step(&action)));
        let err = if r.is_err() { "panic" } else { "" };
        writeln!(out, "{}", dump(gs, err)).unwrap();
        out.flush().unwrap();
    }
}
