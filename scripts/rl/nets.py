"""The policy networks, one definition for the trainers (train_bc.py, train_dagger.py, train_rl.py, screen_items.py)
and the Rust inference (native/rlcore/src/net.rs):

  flat   train_bc.Net: sparse embedding, ReLU MLP, one logit per vocabulary id, value head. BRLW format 1.
  item   ItemNet: the flat network, with the logits of the joker-acquiring ids (buy_joker:<key>,
         replace_joker:<sold>@slot<k>) from one shared scorer over per-joker embeddings (milestone 9's attention
         variant; attention=False is its mean variant). BRLW format 2 (export_weights).

Both take forward(idx, offsets, vals, mask=None) and return (logits, value); ItemNet needs the legal mask (the item
logits are computed for the legal joker-acquiring ids only; every other id keeps the flat head's logit).
`net_from_state(sd)` rebuilds either from a state dict, `export_weights(model, path, schema)` writes either format.
"""
import struct

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

import train_bc as B

FORMAT_FLAT, FORMAT_ITEM = 1, 2


def block(schema, name):
    b = next(x for x in schema["blocks"] if x["name"] == name)
    return b["offset"], len(b["rows"]), len(b["cols"])


def family(schema, name):
    f = next(x for x in schema["vocab"]["families"] if x["name"] == name)
    return f["start"], f["len"]


def item_layout(schema):
    """The schema-derived constants of an ItemNet (stored in its state dict and its weight file)."""
    buy, rep = family(schema, "buy_joker"), family(schema, "replace_joker")
    held, shop, slot = block(schema, "held_jokers"), block(schema, "shop_jokers"), block(schema, "shop_slot_jokers")
    assert rep[1] == held[1] * slot[1] and buy[1] == held[1] == shop[1] == slot[2]
    return [*buy, *rep, slot[1], *held, *shop, *slot]


class ItemNet(nn.Module):
    def __init__(self, obs_dim, n_actions, layout, hidden=(256, 256), d=64, attention=True, s1=128, s2=64, n_value=2):
        super().__init__()
        self.flat = B.Net(obs_dim, n_actions, list(hidden), n_value=n_value)
        self.register_buffer("layout", torch.tensor(layout, dtype=torch.int64))
        self.register_buffer("attention_flag", torch.tensor(int(attention)))
        L = [int(x) for x in layout]
        self.buy, self.rep, self.n_slots = (L[0], L[1]), (L[2], L[3]), L[4]
        self.held, self.shop, self.slots = tuple(L[5:8]), tuple(L[8:11]), tuple(L[11:14])
        nj = self.held[1]
        self.attention = attention
        self.hidden = list(hidden)
        self.E = nn.Embedding(nj, d)
        nn.init.normal_(self.E.weight, std=0.1)
        self.fh = nn.Linear(self.held[2], d)
        self.fs = nn.Linear(self.shop[2], d)
        self.ctx = nn.Linear(hidden[-1], d)
        self.q = nn.Linear(3 * d, d)
        self.k = nn.Linear(d, d)
        self.score = nn.Sequential(nn.Linear(4 * d + 1, s1), nn.ReLU(), nn.Linear(s1, s2), nn.ReLU(), nn.Linear(s2, 1))
        self.fam_bias = nn.Parameter(torch.zeros(2))
        self.d = d

    @property
    def v(self):
        return self.flat.v

    def forward(self, idx, off, vals, mask=None):
        assert mask is not None, "ItemNet needs the legal mask"
        f = self.flat
        h = F.relu(f.emb(idx, off, per_sample_weights=vals) + f.b0)
        for l in f.layers:
            h = F.relu(l(h))
        logits = f.pi(h)
        value = f.v(h.detach() if f.value_detach else h)
        Bn = h.shape[0]
        b0, bl = self.buy
        r0, rl = self.rep
        rr_b, aa_b = torch.nonzero(mask[:, b0:b0 + bl], as_tuple=True)
        rr_s, aa_s = torch.nonzero(mask[:, r0:r0 + rl], as_tuple=True)
        rr = torch.cat([rr_b, rr_s])
        aa = torch.cat([aa_b + b0, aa_s + r0])
        if len(rr) == 0:
            return logits, value
        seg = torch.repeat_interleave(torch.arange(Bn), torch.diff(torch.cat([off, torch.tensor([len(idx)])])))

        def dense(o, n):
            m = (idx >= o) & (idx < o + n)
            out = torch.zeros(Bn, n)
            out[seg[m], idx[m] - o] = vals[m]
            return out

        ho, hr, hc = self.held
        so, sr, sc = self.shop
        lo, lr, lc = self.slots
        held = dense(ho, hr * hc).view(Bn, hr, hc)
        shop = dense(so, sr * sc).view(Bn, sr, sc)
        slotj = dense(lo, lr * lc).view(Bn, lr, lc)
        eh = self.E.weight[None] + self.fh(held)
        es = self.E.weight[None] + self.fs(shop)
        owned = (held[:, :, 0] > 0).float()
        c = self.ctx(h)
        is_swap = aa >= r0
        zero = torch.zeros_like(aa)
        buy_key = torch.where(is_swap, zero, aa - b0)
        sold = torch.where(is_swap, (aa - r0) // self.n_slots, zero)
        slot = torch.where(is_swap, (aa - r0) % self.n_slots, zero)
        in_key = torch.where(is_swap, slotj[rr, slot].argmax(1), buy_key)
        e_in = es[rr, in_key]
        e_out = eh[rr, sold] * is_swap[:, None].float()
        cr = c[rr]
        own = owned[rr].clone()
        own[torch.arange(len(rr)), sold] *= (~is_swap).float()  # the sold joker is no longer owned
        if self.attention:
            q = self.q(torch.cat([e_in, e_out, cr], 1))
            kk = self.k(eh[rr])
            att = (kk @ q[:, :, None])[:, :, 0] / np.sqrt(self.d)
            att = att.masked_fill(own == 0, -1e9)
            w = torch.softmax(att, 1) * (own.sum(1, keepdim=True) > 0).float()
            summ = (w[:, :, None] * eh[rr]).sum(1)
        else:
            summ = (own[:, :, None] * eh[rr]).sum(1) / own.sum(1, keepdim=True).clamp(min=1)
        x = torch.cat([cr, e_in, e_out, summ, is_swap[:, None].float()], 1)
        s = self.score(x)[:, 0] + self.fam_bias[is_swap.long()]
        logits = logits.index_put((rr, aa), s)
        return logits, value


def is_item_state(sd):
    return "E.weight" in sd


def net_from_state(sd):
    """A flat Net or an ItemNet with the shapes of state dict `sd`, loaded with it."""
    pre = "flat." if is_item_state(sd) else ""
    n_layers = len([k for k in sd if k.startswith(pre + "layers.") and k.endswith(".weight")])
    hidden = [sd[pre + "emb.weight"].shape[1]] + [sd[f"{pre}layers.{i}.weight"].shape[0] for i in range(n_layers)]
    obs_dim, n_actions, n_value = sd[pre + "emb.weight"].shape[0], sd[pre + "pi.weight"].shape[0], sd[pre + "v.weight"].shape[0]
    if pre:
        m = ItemNet(obs_dim, n_actions, sd["layout"].tolist(), hidden, d=sd["E.weight"].shape[1], attention=bool(int(sd["attention_flag"])),
                    s1=sd["score.0.weight"].shape[0], s2=sd["score.2.weight"].shape[0], n_value=n_value)
    else:
        m = B.Net(obs_dim, n_actions, hidden, n_value=n_value)
    m.load_state_dict(sd)
    return m


def export_weights(model, path, schema_hash, obs_dim, n_actions):
    """BRLW format 1 (flat) or 2 (item), see native/rlcore/src/net.rs."""
    if not isinstance(model, ItemNet):
        return B.export_weights(model, path, schema_hash, obs_dim, n_actions)
    f_ = model.flat
    w = lambda f, t: f.write(t.detach().cpu().float().contiguous().numpy().astype("<f4").tobytes())
    with open(path, "wb") as f:
        f.write(b"BRLW")
        f.write(struct.pack("<IQII", FORMAT_ITEM, schema_hash, obs_dim, n_actions))
        f.write(struct.pack("<I", len(f_.hidden)))
        f.write(struct.pack(f"<{len(f_.hidden)}I", *f_.hidden))
        f.write(struct.pack("<I", f_.v.out_features))
        w(f, f_.emb.weight)
        w(f, f_.b0)
        for l in f_.layers:
            w(f, l.weight)
            w(f, l.bias)
        w(f, f_.pi.weight)
        w(f, f_.pi.bias)
        w(f, f_.v.weight)
        w(f, f_.v.bias)
        f.write(struct.pack("<4I", model.d, int(model.attention), model.score[0].out_features, model.score[2].out_features))
        f.write(struct.pack("<14I", *[int(x) for x in model.layout.tolist()]))
        w(f, model.E.weight)
        for lin in (model.fh, model.fs, model.ctx, model.q, model.k, model.score[0], model.score[2], model.score[4]):
            w(f, lin.weight)
            w(f, lin.bias)
        w(f, model.fam_bias)
