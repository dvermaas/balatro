//! Policy network inference (used by rlenv and by jevlab's JEVLAB_RLPOLICY): sparse first layer, ReLU MLP trunk, masked policy logits, value head.
//!
//! Weight file (little endian), written by `scripts/rl/train_bc.py` (`export_weights`):
//!
//! ```text
//! b"BRLW"  u32 format (=1)  u64 schema_hash  u32 obs_dim  u32 n_actions  u32 n_layers (L >= 1)  u32 hidden[L]
//! u32 n_value (=2)
//! f32 W0t[obs_dim][hidden0]   first layer, input-major (row i = the weights of observation entry i)
//! f32 b0[hidden0]
//! for l in 1..L:  f32 Wl[hidden_l][hidden_{l-1}]  f32 bl[hidden_l]      (PyTorch nn.Linear layout: out x in)
//! f32 Wp[n_actions][hidden_{L-1}]  f32 bp[n_actions]                     policy logits
//! f32 Wv[n_value][hidden_{L-1}]    f32 bv[n_value]                       value: [0] logit of P(win),
//!                                                                        [1] expected final blinds / 24
//! ```
//!
//! Format 2 (`scripts/rl/nets.py`, `export_item`): the format-1 body, then an item scorer that replaces the logits of
//! the joker-acquiring ids (buy_joker, replace_joker) with one shared MLP over per-joker embeddings:
//!
//! ```text
//! u32 d  u32 attention (0 mean / 1 attention)  u32 s1  u32 s2           embedding size, scorer hidden sizes
//! u32 buy_start buy_len rep_start rep_len n_slots                       vocabulary ranges (replace id = rep_start + sold * n_slots + slot)
//! u32 held_off held_rows held_cols  shop_off shop_rows shop_cols  slot_off slot_rows slot_cols    observation blocks
//! f32 E[held_rows][d]                                                   per-key joker embedding
//! f32 Wh[d][held_cols] bh[d]   Ws[d][shop_cols] bs[d]   Wc[d][hidden_{L-1}] bc[d]
//! f32 Wq[d][3d] bq[d]   Wk[d][d] bk[d]
//! f32 W1[s1][4d+1] b1[s1]   W2[s2][s1] b2[s2]   W3[1][s2] b3[1]   fam_bias[2] (buy, swap)
//! ```
//!
//! For a legal joker-acquiring id: e_in = E[k] + Ws shop[k] + bs for the shop joker k (a buy's key, or the joker in a
//! swap's shop slot: the first maximum of that slot's row); e_out = E[s] + Wh held[s] + bh for a swap's sold joker s,
//! else 0; the owned jokers (held count > 0, the sold one excluded) summarised as the mean of their e (attention 0)
//! or softmax_j((Wk e_j + bk) . (Wq [e_in, e_out, c] + bq) / sqrt(d)) weighted e_j (attention 1; 0 if none);
//! c = Wc trunk + bc; logit = MLP([c, e_in, e_out, summary, is_swap]) + fam_bias[is_swap]. Other ids keep the
//! format-1 logits.
//!
//! Every hidden layer is followed by ReLU. Loading refuses a file whose schema hash, obs_dim or n_actions differ
//! from this build's rlcore schema.

use std::io::Read;

pub const MAGIC: &[u8; 4] = b"BRLW";
pub const FORMAT: u32 = 1;
pub const FORMAT_ITEM: u32 = 2;

pub struct Net {
    pub obs_dim: usize,
    pub n_actions: usize,
    pub hidden: Vec<usize>,
    pub n_value: usize,
    w0t: Vec<f32>,
    b0: Vec<f32>,
    layers: Vec<(Vec<f32>, Vec<f32>)>,
    wp: Vec<f32>,
    bp: Vec<f32>,
    wv: Vec<f32>,
    bv: Vec<f32>,
    item: Option<Item>,
}

/// The format-2 item scorer (see the module docs).
pub struct Item {
    d: usize,
    attention: bool,
    buy: (usize, usize),
    rep: (usize, usize),
    n_slots: usize,
    held: (usize, usize, usize),
    shop: (usize, usize, usize),
    slot: (usize, usize, usize),
    e: Vec<f32>,
    wh: Vec<f32>,
    bh: Vec<f32>,
    ws: Vec<f32>,
    bs: Vec<f32>,
    wc: Vec<f32>,
    bc: Vec<f32>,
    wq: Vec<f32>,
    bq: Vec<f32>,
    wk: Vec<f32>,
    bk: Vec<f32>,
    w1: Vec<f32>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: Vec<f32>,
    w3: Vec<f32>,
    b3: Vec<f32>,
    fam: Vec<f32>,
}

/// Activations of the last hidden layer.
pub struct Trunk(pub Vec<f32>);

fn rd_u32(b: &[u8], p: &mut usize) -> Result<u32, String> {
    let v = b.get(*p..*p + 4).ok_or("truncated weight file")?;
    *p += 4;
    Ok(u32::from_le_bytes(v.try_into().unwrap()))
}

fn rd_f32s(b: &[u8], p: &mut usize, n: usize) -> Result<Vec<f32>, String> {
    let v = b.get(*p..*p + 4 * n).ok_or("truncated weight file")?;
    *p += 4 * n;
    Ok(v.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
}

impl Net {
    pub fn load(path: &std::path::Path) -> Result<Net, String> {
        let mut b = Vec::new();
        std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut b)).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(&b)
    }

    pub fn from_bytes(b: &[u8]) -> Result<Net, String> {
        if b.len() < 4 || &b[..4] != MAGIC {
            return Err("not a BRLW weight file".into());
        }
        let mut p = 4;
        let fmt = rd_u32(b, &mut p)?;
        if fmt != FORMAT && fmt != FORMAT_ITEM {
            return Err(format!("weight format {fmt}, expected {FORMAT} or {FORMAT_ITEM}"));
        }
        let hash = u64::from_le_bytes(b.get(p..p + 8).ok_or("truncated")?.try_into().unwrap());
        p += 8;
        if hash != crate::schema_hash() {
            return Err(format!("weights are for schema hash {hash:016x}, this build is {:016x}", crate::schema_hash()));
        }
        let obs_dim = rd_u32(b, &mut p)? as usize;
        let n_actions = rd_u32(b, &mut p)? as usize;
        if obs_dim != crate::obs_dim() || n_actions != crate::n_actions() {
            return Err(format!("weights are {obs_dim} x {n_actions}, schema is {} x {}", crate::obs_dim(), crate::n_actions()));
        }
        let nl = rd_u32(b, &mut p)? as usize;
        if nl == 0 || nl > 16 {
            return Err(format!("bad layer count {nl}"));
        }
        let hidden: Vec<usize> = (0..nl).map(|_| rd_u32(b, &mut p).map(|x| x as usize)).collect::<Result<_, _>>()?;
        let n_value = rd_u32(b, &mut p)? as usize;
        let w0t = rd_f32s(b, &mut p, obs_dim * hidden[0])?;
        let b0 = rd_f32s(b, &mut p, hidden[0])?;
        let mut layers = Vec::new();
        for l in 1..nl {
            let w = rd_f32s(b, &mut p, hidden[l] * hidden[l - 1])?;
            let bb = rd_f32s(b, &mut p, hidden[l])?;
            layers.push((w, bb));
        }
        let h = hidden[nl - 1];
        let wp = rd_f32s(b, &mut p, n_actions * h)?;
        let bp = rd_f32s(b, &mut p, n_actions)?;
        let wv = rd_f32s(b, &mut p, n_value * h)?;
        let bv = rd_f32s(b, &mut p, n_value)?;
        let item = if fmt == FORMAT_ITEM { Some(Item::read(b, &mut p, h, n_actions, obs_dim)?) } else { None };
        if p != b.len() {
            return Err(format!("{} trailing bytes in weight file", b.len() - p));
        }
        Ok(Net { obs_dim, n_actions, hidden, n_value, w0t, b0, layers, wp, bp, wv, bv, item })
    }

    /// Trunk activations for a sparse observation.
    pub fn trunk(&self, idx: &[u16], val: &[f32]) -> Trunk {
        let h0 = self.hidden[0];
        let mut h = self.b0.clone();
        for (&i, &v) in idx.iter().zip(val) {
            let row = &self.w0t[i as usize * h0..(i as usize + 1) * h0];
            for (a, w) in h.iter_mut().zip(row) {
                *a += v * w;
            }
        }
        h.iter_mut().for_each(|x| *x = x.max(0.0));
        for (w, bb) in &self.layers {
            let n_in = h.len();
            let mut out = bb.clone();
            for (o, row) in out.iter_mut().zip(w.chunks_exact(n_in)) {
                *o += dot(row, &h);
            }
            out.iter_mut().for_each(|x| *x = x.max(0.0));
            h = out;
        }
        Trunk(h)
    }

    /// The policy logit of vocabulary id `a`.
    pub fn logit(&self, t: &Trunk, a: usize) -> f32 {
        let n = t.0.len();
        self.bp[a] + dot(&self.wp[a * n..(a + 1) * n], &t.0)
    }

    /// All `n_actions` logits (unmasked).
    pub fn logits(&self, t: &Trunk) -> Vec<f32> {
        (0..self.n_actions).map(|a| self.logit(t, a)).collect()
    }

    /// One decision over the legal ids (ascending): argmax (`temp` None; first maximum) or a sample from
    /// softmax(logits / temp) driven by `u` uniform in [0, 1). Returns (id, log-probability of id under the
    /// distribution used (argmax: under temperature 1), [P(win), expected final blinds]).
    pub fn choose(&self, idx: &[u16], val: &[f32], legal: &[u16], temp: Option<f32>, u: f64) -> (usize, f32, [f32; 2]) {
        let t = self.trunk(idx, val);
        let logits: Vec<f32> = match &self.item {
            None => legal.iter().map(|&a| self.logit(&t, a as usize)).collect(),
            Some(it) => {
                let mut l: Vec<f32> = legal.iter().map(|&a| self.logit(&t, a as usize)).collect();
                it.overwrite(&t, idx, val, legal, &mut l);
                l
            }
        };
        let v = self.value(&t);
        let value = [1.0 / (1.0 + (-v[0]).exp()), v.get(1).copied().unwrap_or(0.0) * 24.0];
        let tt = temp.unwrap_or(1.0).max(1e-6);
        let m = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let ex: Vec<f64> = logits.iter().map(|&l| (((l - m) / tt) as f64).exp()).collect();
        let z: f64 = ex.iter().sum();
        let k = match temp {
            None => {
                let mut best = 0;
                for i in 1..logits.len() {
                    if logits[i] > logits[best] {
                        best = i;
                    }
                }
                best
            }
            Some(_) => {
                let mut r = u * z;
                let mut k = ex.len() - 1;
                for (i, e) in ex.iter().enumerate() {
                    if r < *e {
                        k = i;
                        break;
                    }
                    r -= e;
                }
                k
            }
        };
        (legal[k] as usize, (ex[k] / z).ln() as f32, value)
    }

    /// Whether this is a format-2 network (item scorer).
    pub fn is_item(&self) -> bool {
        self.item.is_some()
    }

    /// All `n_actions` logits for a state with legal ids `legal`: the format-1 logits, with a format-2 network's
    /// item scorer replacing those of the legal joker-acquiring ids (what `choose` ranks).
    pub fn logits_legal(&self, idx: &[u16], val: &[f32], legal: &[u16]) -> Vec<f32> {
        let t = self.trunk(idx, val);
        let mut all = self.logits(&t);
        if let Some(it) = &self.item {
            let mut l: Vec<f32> = legal.iter().map(|&a| all[a as usize]).collect();
            it.overwrite(&t, idx, val, legal, &mut l);
            for (&a, x) in legal.iter().zip(l) {
                all[a as usize] = x;
            }
        }
        all
    }

    /// Raw value outputs: [win logit, final blinds / 24].
    pub fn value(&self, t: &Trunk) -> Vec<f32> {
        let n = t.0.len();
        (0..self.n_value).map(|k| self.bv[k] + dot(&self.wv[k * n..(k + 1) * n], &t.0)).collect()
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    // 8 accumulators: lets the compiler vectorise, and is no worse for accuracy than one running sum
    let mut acc = [0f32; 8];
    let mut ca = a.chunks_exact(8);
    let mut cb = b.chunks_exact(8);
    for (x, y) in (&mut ca).zip(&mut cb) {
        for k in 0..8 {
            acc[k] += x[k] * y[k];
        }
    }
    let mut s: f32 = acc.iter().sum();
    for (x, y) in ca.remainder().iter().zip(cb.remainder()) {
        s += x * y;
    }
    s
}

fn linear(w: &[f32], b: &[f32], x: &[f32]) -> Vec<f32> {
    let n = x.len();
    b.iter().zip(w.chunks_exact(n)).map(|(bb, row)| bb + dot(row, x)).collect()
}

fn relu(mut v: Vec<f32>) -> Vec<f32> {
    v.iter_mut().for_each(|x| *x = x.max(0.0));
    v
}

impl Item {
    fn read(b: &[u8], p: &mut usize, h: usize, n_actions: usize, obs_dim: usize) -> Result<Item, String> {
        let mut u = || rd_u32(b, p).map(|x| x as usize);
        let (d, att, s1, s2) = (u()?, u()?, u()?, u()?);
        let (bs0, bl, rs0, rl, n_slots) = (u()?, u()?, u()?, u()?, u()?);
        let held = (u()?, u()?, u()?);
        let shop = (u()?, u()?, u()?);
        let slot = (u()?, u()?, u()?);
        let nj = held.1;
        if d == 0 || d > 4096 || s1 == 0 || s2 == 0 || bs0 + bl > n_actions || rs0 + rl > n_actions || rl != nj * n_slots || bl != nj
            || shop.1 != nj || slot.2 != nj || slot.1 != n_slots
            || [held, shop, slot].iter().any(|&(o, r, c)| o + r * c > obs_dim)
        {
            return Err("bad item section in weight file".into());
        }
        let mut f = |n: usize| rd_f32s(b, p, n);
        Ok(Item {
            d,
            attention: att != 0,
            buy: (bs0, bl),
            rep: (rs0, rl),
            n_slots,
            held,
            shop,
            slot,
            e: f(nj * d)?,
            wh: f(d * held.2)?,
            bh: f(d)?,
            ws: f(d * shop.2)?,
            bs: f(d)?,
            wc: f(d * h)?,
            bc: f(d)?,
            wq: f(d * 3 * d)?,
            bq: f(d)?,
            wk: f(d * d)?,
            bk: f(d)?,
            w1: f(s1 * (4 * d + 1))?,
            b1: f(s1)?,
            w2: f(s2 * s1)?,
            b2: f(s2)?,
            w3: f(s2)?,
            b3: f(1)?,
            fam: f(2)?,
        })
    }

    /// e = E[key] + W row + b
    fn embed(&self, key: usize, w: &[f32], bias: &[f32], row: &[f32]) -> Vec<f32> {
        let d = self.d;
        let mut e = linear(w, bias, row);
        for (x, y) in e.iter_mut().zip(&self.e[key * d..(key + 1) * d]) {
            *x += y;
        }
        e
    }

    /// Replace the logits `l` of the joker-acquiring ids among `legal` (same order) with the scorer's.
    fn overwrite(&self, t: &Trunk, idx: &[u16], val: &[f32], legal: &[u16], l: &mut [f32]) {
        let in_buy = |a: usize| a >= self.buy.0 && a < self.buy.0 + self.buy.1;
        let in_rep = |a: usize| a >= self.rep.0 && a < self.rep.0 + self.rep.1;
        if !legal.iter().any(|&a| in_buy(a as usize) || in_rep(a as usize)) {
            return;
        }
        let d = self.d;
        let nj = self.held.1;
        let (ho, _, hc) = self.held;
        let (so, _, sc) = self.shop;
        let (lo, lr, lc) = self.slot;
        let mut held = vec![0f32; nj * hc];
        let mut shop = vec![0f32; nj * sc];
        let mut slots = vec![0f32; lr * lc];
        for (&i, &v) in idx.iter().zip(val) {
            let i = i as usize;
            if i >= ho && i < ho + nj * hc {
                held[i - ho] = v;
            } else if i >= so && i < so + nj * sc {
                shop[i - so] = v;
            } else if i >= lo && i < lo + lr * lc {
                slots[i - lo] = v;
            }
        }
        let owned: Vec<usize> = (0..nj).filter(|&j| held[j * hc] > 0.0).collect();
        let eh: Vec<Vec<f32>> = owned.iter().map(|&j| self.embed(j, &self.wh, &self.bh, &held[j * hc..(j + 1) * hc])).collect();
        let keys: Vec<Vec<f32>> = if self.attention { eh.iter().map(|e| linear(&self.wk, &self.bk, e)).collect() } else { Vec::new() };
        let c = linear(&self.wc, &self.bc, &t.0);
        let scale = 1.0 / (d as f32).sqrt();
        for (k, &a) in legal.iter().enumerate() {
            let a = a as usize;
            let (swap, in_key, sold) = if in_buy(a) {
                (false, a - self.buy.0, usize::MAX)
            } else if in_rep(a) {
                let r = a - self.rep.0;
                let (s, slot) = (r / self.n_slots, r % self.n_slots);
                let row = &slots[slot * lc..(slot + 1) * lc];
                let mut best = 0;
                for j in 1..lc {
                    if row[j] > row[best] {
                        best = j;
                    }
                }
                (true, best, s)
            } else {
                continue;
            };
            let e_in = self.embed(in_key, &self.ws, &self.bs, &shop[in_key * sc..(in_key + 1) * sc]);
            let e_out = if swap { self.embed(sold, &self.wh, &self.bh, &held[sold * hc..(sold + 1) * hc]) } else { vec![0f32; d] };
            let mut summ = vec![0f32; d];
            let members: Vec<usize> = (0..owned.len()).filter(|&m| owned[m] != sold).collect();
            if !members.is_empty() {
                if self.attention {
                    let mut qin = e_in.clone();
                    qin.extend_from_slice(&e_out);
                    qin.extend_from_slice(&c);
                    let q = linear(&self.wq, &self.bq, &qin);
                    let att: Vec<f32> = members.iter().map(|&m| dot(&keys[m], &q) * scale).collect();
                    let mx = att.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let ex: Vec<f32> = att.iter().map(|x| (x - mx).exp()).collect();
                    let z: f32 = ex.iter().sum();
                    for (&m, w) in members.iter().zip(&ex) {
                        for (s, y) in summ.iter_mut().zip(&eh[m]) {
                            *s += w / z * y;
                        }
                    }
                } else {
                    for &m in &members {
                        for (s, y) in summ.iter_mut().zip(&eh[m]) {
                            *s += y;
                        }
                    }
                    let n = members.len() as f32;
                    summ.iter_mut().for_each(|s| *s /= n);
                }
            }
            let mut x = c.clone();
            x.extend_from_slice(&e_in);
            x.extend_from_slice(&e_out);
            x.extend_from_slice(&summ);
            x.push(if swap { 1.0 } else { 0.0 });
            let h1 = relu(linear(&self.w1, &self.b1, &x));
            let h2 = relu(linear(&self.w2, &self.b2, &h1));
            l[k] = self.b3[0] + dot(&self.w3, &h2) + self.fam[swap as usize];
        }
    }
}
