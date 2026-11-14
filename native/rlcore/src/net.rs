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
//! Every hidden layer is followed by ReLU. Loading refuses a file whose schema hash, obs_dim or n_actions differ
//! from this build's rlcore schema.

use std::io::Read;

pub const MAGIC: &[u8; 4] = b"BRLW";
pub const FORMAT: u32 = 1;

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
        if fmt != FORMAT {
            return Err(format!("weight format {fmt}, expected {FORMAT}"));
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
        if p != b.len() {
            return Err(format!("{} trailing bytes in weight file", b.len() - p));
        }
        Ok(Net { obs_dim, n_actions, hidden, n_value, w0t, b0, layers, wp, bp, wv, bv })
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
        let logits: Vec<f32> = legal.iter().map(|&a| self.logit(&t, a as usize)).collect();
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
