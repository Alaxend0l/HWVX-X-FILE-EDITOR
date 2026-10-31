//! Walk a car pack's object graph and classify every field, so the blob can
//! be byte-swapped between the PS2 prototype (little-endian) and GameCube
//! (big-endian).
//!
//! This mirrors the claim walkers of HWVX-SDL `src/game/Port_Swap.c` (which
//! the SDL port uses to swap GameCube packs on load) for everything a car
//! pack contains: CarAInfo and below, slave triggers, event sequences,
//! sounds, animations, AI scripts, and the HUD interface. Fields not claimed
//! as u16 or bytes are 32-bit (u32/s32/f32/pointers), as in `Port_Swap.c`.
//! It also collects the geometry each object points at ("geometry roots").

use std::collections::{BTreeSet, HashMap};

use crate::blob::{Blob, Kind, SwapMap};
use crate::format::Endian;

const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Claim {
    Default,
    Ptr,
    U16,
    Bytes,
    U32,
    TexRef,
    SndRef,
    RefRel,
    Str,
}

/// Result of a walk.
pub struct CarWalk {
    pub swap: SwapMap,
    /// Targets of pointer slots that hold geometry (local), from the objects
    /// that own geometry (motion parts, car parts, thing records, attached
    /// models, projectiles, the shadow at CarAInfo+0x54).
    pub geometry_roots: BTreeSet<u32>,
    /// Blob offset of the root CarAInfo.
    pub car_info: Option<u32>,
}

struct W<'a> {
    b: &'a [u8],
    e: Endian,
    claim: Vec<Claim>,
    pointers: Vec<u32>,
    geom: BTreeSet<u32>,
    strs: BTreeSet<u32>,
    es_depth: u32,
    sl_depth: u32,
}

const KEYS_ACTIVE: u8 = 0;
const KEYS_VEC4: u8 = 1;
const KEYS_COLOR: u8 = 2;
const KEYS_UV: u8 = 3;
const KEYS_F32: u8 = 4;
const KEYS_POINT: u8 = 5;

impl<'a> W<'a> {
    fn size(&self) -> u32 {
        self.b.len() as u32
    }
    fn inb(&self, off: u32, n: u32) -> bool {
        off != NONE && off < self.size() && n <= self.size() - off
    }
    fn word(&self, off: u32) -> u32 {
        if self.inb(off, 4) {
            self.e.u32_at(self.b, off as usize).unwrap()
        } else {
            0
        }
    }
    fn f32(&self, off: u32) -> f32 {
        f32::from_bits(self.word(off))
    }
    fn byte(&self, off: u32) -> u8 {
        if self.inb(off, 1) {
            self.b[off as usize]
        } else {
            0
        }
    }
    fn claim_at(&self, off: u32) -> Claim {
        if self.inb(off, 1) {
            self.claim[off as usize]
        } else {
            Claim::Default
        }
    }
    fn target(&self, off: u32) -> u32 {
        if !self.inb(off, 4) || self.claim[off as usize] != Claim::Ptr {
            return NONE;
        }
        let v = self.word(off);
        if v < self.size() {
            v
        } else {
            NONE
        }
    }
    fn cl(&mut self, off: u32, n: u32, kind: Claim) {
        if off == NONE || !self.inb(off, n) {
            return;
        }
        for i in off..off + n {
            let c = &mut self.claim[i as usize];
            if !matches!(*c, Claim::Ptr | Claim::RefRel) {
                *c = kind;
            }
        }
    }
    fn u16(&mut self, off: u32) {
        self.cl(off, 2, Claim::U16)
    }
    fn bytes(&mut self, off: u32, n: u32) {
        self.cl(off, n, Claim::Bytes)
    }
    fn geom_root(&mut self, t: u32) {
        if t != NONE {
            self.geom.insert(t);
        }
    }

    // ---- key tracks -------------------------------------------------------
    fn key_track(&mut self, track: u32, stride: u32, bytes_at: u32) {
        let keys = self.target(track + 4);
        if keys == NONE {
            return;
        }
        let n = self.word(track).min(0x10000);
        for i in 0..n {
            let k = keys + i * stride;
            self.bytes(k + bytes_at, 4);
            let mut w = 0;
            while w < stride {
                if w != bytes_at && self.inb(k + w, 4) && self.claim_at(k + w) == Claim::Default {
                    self.cl(k + w, 4, Claim::U32);
                }
                w += 4;
            }
        }
    }
    fn track(&mut self, t: u32, kind: u8) {
        match kind {
            KEYS_ACTIVE => self.key_track(t, 8, 0),
            KEYS_VEC4 => self.key_track(t, 0x20, 0x10),
            KEYS_UV => self.key_track(t, 0x28, 0x20),
            KEYS_F32 => self.key_track(t, 0xC, 4),
            KEYS_POINT => self.key_track(t, 0x10, 0xC),
            _ => {
                self.key_track(t, 0xC, 0);
                self.key_track(t, 0xC, 4);
            }
        }
    }

    // ---- sounds -----------------------------------------------------------
    fn sound(&mut self, off: u32) {
        if off != NONE {
            self.bytes(off, 8);
        }
    }
    fn sound_seq(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        self.bytes(off + 4, 2);
        self.u16(off + 6);
        let keys = self.target(off + 0xC);
        if keys == NONE {
            return;
        }
        let n = self.word(off + 8).min(0x1000);
        for i in 0..n {
            let k = keys + i * 0x10;
            let lst = self.target(k + 0xC);
            let ns = self.word(k + 8).min(0x100);
            self.bytes(k + 4, 4);
            if lst != NONE {
                for j in 0..ns {
                    let s = self.target(lst + j * 4);
                    self.sound(s);
                }
            }
        }
    }

    fn camera_list(&mut self, off: u32) {
        let lst = self.target(off + 4);
        if lst == NONE {
            return;
        }
        let n = self.word(off).min(0x40);
        for i in 0..n {
            let e = lst + i * 0xC;
            self.bytes(e, 4);
            let bp = self.target(e + 4);
            if self.byte(e) == 4 && bp != NONE {
                self.u16(bp + 0x14);
                self.bytes(bp + 0x16, 2);
            }
        }
    }

    fn thing_record(&mut self, rec: u32) {
        if rec == NONE || self.claim_at(rec + 4) == Claim::Bytes {
            return;
        }
        if self.byte(rec + 4) == 1 {
            let m = self.target(rec);
            self.motion_anim(m);
        } else {
            let g = self.target(rec);
            self.geom_root(g);
        }
        self.bytes(rec + 4, 4);
        let info = self.target(rec + 0x10);
        if info != NONE {
            self.bytes(info + 0x10, 2);
        }
        self.slave_list(rec + 0x18);
    }

    fn ai_state(&mut self, st: u32, depth: u32) {
        if st == NONE || depth > 32 {
            return;
        }
        let info = self.target(st);
        if info != NONE {
            self.bytes(info, 8);
            let lim = self.target(info + 0x58);
            if lim != NONE {
                self.bytes(lim + 0x10, 4);
            }
        }
        let ch = self.target(st + 8);
        let n = self.word(st + 4).min(0x100);
        if ch != NONE {
            for i in 0..n {
                self.ai_state(ch + i * 0xC, depth + 1);
            }
        }
    }

    fn ai_script(&mut self, sc: u32) {
        if sc == NONE {
            return;
        }
        self.bytes(sc + 0x24, 4);
        self.ai_state(sc + 0x28, 0);
    }

    fn line_def(&mut self, d: u32) {
        if d == NONE || self.claim_at(d) == Claim::Bytes {
            return;
        }
        self.bytes(d, 4);
        self.track(d + 0xC, KEYS_F32);
        self.track(d + 0x14, KEYS_COLOR);
    }

    fn emitter(&mut self, off: u32) {
        if off == NONE || self.claim_at(off) == Claim::Bytes {
            return;
        }
        self.bytes(off, 4);
        let s = self.target(off + 0x20);
        self.sound_seq(s);
    }

    fn event_seq(&mut self, script: u32) {
        if script == NONE || self.es_depth > 4 {
            return;
        }
        self.es_depth += 1;
        let entries = self.target(script + 4);
        let n = if entries != NONE { self.word(script).min(0x400) } else { 0 };
        for i in 0..n {
            let e = entries + i * 0xC;
            let a = self.target(e + 4);
            let b = self.target(e + 8);
            let ty = self.word(e);
            match ty {
                1 | 2 | 3 | 4 | 7 | 29 => {
                    if b != NONE && self.claim_at(b + 0x20) != Claim::Bytes {
                        let adv = self.target(b + 0x28);
                        let info = self.target(b + 0x2C);
                        self.bytes(b + 0x20, 4);
                        if self.byte(b + 0x21) == 2 && adv != NONE {
                            let d = self.target(adv + 0xC);
                            self.bytes(adv + 8, 4);
                            if d != NONE && matches!(self.byte(adv + 9), 1 | 2) {
                                self.bytes(d + 0x10, 4);
                            }
                        }
                        if info != NONE && self.byte(b + 0x22) == 4 {
                            self.bytes(info + 0x10, 4);
                        }
                    }
                    if a != NONE {
                        match ty {
                            3 => self.event_seq(a),
                            4 => self.emitter(a),
                            7 => self.thing_record(a),
                            _ => {}
                        }
                    }
                    if ty == 2 && a != NONE && self.claim_at(a) != Claim::Bytes {
                        let g = self.target(a + 0x10);
                        self.geom_root(g);
                        let tgt = self.target(a + 0xA8);
                        self.bytes(a, 0xC);
                        self.track(a + 0x8C, KEYS_VEC4);
                        self.track(a + 0x94, KEYS_COLOR);
                        self.slave_list(a + 0x9C);
                        if tgt != NONE {
                            self.u16(tgt);
                            self.u16(tgt + 2);
                        }
                    }
                    if ty == 1 && a != NONE {
                        let anim = self.target(a + 4);
                        let kind = self.byte(a);
                        self.bytes(a, 4);
                        if kind <= 2 {
                            self.track(a + 0x14, KEYS_COLOR);
                        }
                        if kind == 2 {
                            let fx = self.target(a + 0x20);
                            self.line_def(anim);
                            if fx != NONE {
                                for k in 0..self.word(a + 0x1C).min(0x40) {
                                    let l = self.target(fx + k * 4);
                                    self.line_def(l);
                                }
                            }
                        } else if anim != NONE {
                            self.bytes(anim, 4);
                            if kind <= 1 {
                                self.track(anim + 0x28, if kind == 0 { KEYS_VEC4 } else { KEYS_F32 });
                            }
                        }
                    }
                }
                5 | 6 | 8 | 10 | 25 | 26 => {
                    if b != NONE {
                        self.bytes(b + 0x18, 4);
                    }
                    if a != NONE {
                        match ty {
                            5 => {
                                self.bytes(a, 8);
                                let dm = self.target(a + 0x1C);
                                if dm != NONE {
                                    self.bytes(dm, 4);
                                }
                                let c = self.target(a + 0x20);
                                if c != NONE {
                                    self.bytes(c, 4);
                                    self.track(c + 4, KEYS_COLOR);
                                }
                                self.track(a + 0x2C, KEYS_POINT);
                            }
                            6 => self.thing_record(a),
                            10 => self.bytes(a, 4),
                            26 => self.sound_seq(a),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        self.es_depth -= 1;
    }

    fn is_slot_or_null(&self, off: u32) -> bool {
        self.inb(off, 4) && (self.claim_at(off) == Claim::Ptr || self.word(off) == 0)
    }

    fn is_event_seq(&self, script: u32) -> bool {
        if script == NONE || !self.inb(script, 8) {
            return false;
        }
        let n = self.word(script);
        let entries = self.target(script + 4);
        if n == 0 || n > 0x100 || entries == NONE || !self.inb(entries, n * 0xC) {
            return false;
        }
        let mut em = 0;
        for i in 0..n {
            let e = entries + i * 0xC;
            let ty = self.word(e);
            if ty > 40 {
                return false;
            }
            if matches!(ty, 1..=8 | 10 | 25 | 26 | 29) {
                if !self.is_slot_or_null(e + 4) || self.target(e + 8) == NONE {
                    return false;
                }
                em += 1;
            }
        }
        em != 0
    }

    fn slave_data(&mut self, ty: u8, data: u32) {
        if data == NONE {
            return;
        }
        match ty {
            1 => self.event_seq(data),
            2 => self.emitter(data),
            9 => {
                self.bytes(data, 4);
                for o in [0x1C, 0x20] {
                    let t = self.target(data + o);
                    if t != NONE {
                        self.bytes(t, 4);
                    }
                }
            }
            3 => self.sound_seq(data),
            5 | 6 => {
                self.bytes(data, 4);
                let g = self.target(data + 8);
                self.geom_root(g);
                self.track(data + 0xC, KEYS_VEC4);
                self.track(data + 0x14, KEYS_VEC4);
                self.track(data + 0x1C, KEYS_COLOR);
            }
            7 => {
                self.bytes(data + 0x20, 4);
                self.track(data + 0x44, KEYS_COLOR);
                self.track(data + 0x4C, KEYS_COLOR);
            }
            8 => self.motion_anim(data),
            10 => self.slave_list(data + 4),
            _ => {}
        }
    }

    fn slave_list(&mut self, off: u32) {
        let t = self.target(off + 4);
        if self.sl_depth > 8 || t == NONE {
            return;
        }
        self.sl_depth += 1;
        let n = self.word(off).min(0x400);
        for i in 0..n {
            let e = t + i * 0x20;
            if self.claim_at(e + 0x10) == Claim::Bytes {
                continue;
            }
            self.bytes(e + 0x10, 2);
            self.u16(e + 0x12);
            self.bytes(e + 0x1C, 4);
            let (ty, d) = (self.byte(e + 0x10), self.target(e + 0x18));
            self.slave_data(ty, d);
        }
        self.sl_depth -= 1;
    }

    fn key_anim(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        let flags = self.target(off + 0x1C);
        let n = self.word(off + 0x10).min(0x100);
        if flags != NONE {
            for i in 0..n {
                self.u16(flags + i * 2);
            }
        }
    }

    fn motion_anim(&mut self, off: u32) {
        if off == NONE || self.claim_at(off + 0x1C) == Claim::Bytes {
            return;
        }
        self.bytes(off + 0x1C, 4);
        let k = self.target(off + 0x10);
        self.key_anim(k);
        self.slave_list(off + 0x2C);
        let p = self.target(off + 0xC);
        let n = self.word(off + 8).min(0x400);
        if p != NONE {
            for i in 0..n {
                self.u16(p + i * 8 + 4);
                self.u16(p + i * 8 + 6);
                let g = self.target(p + i * 8);
                self.geom_root(g);
            }
        }
        let g = self.target(off + 4);
        let ng = self.word(off);
        if g != NONE && ng < 0x100 {
            for i in 0..ng {
                let t = self.target(g + 4 * i);
                self.geom_root(t);
            }
        }
        let sk = self.target(off + 0x14);
        if sk != NONE {
            let bones = self.target(sk + 0x10);
            let n = self.word(sk).min(0x100);
            if bones != NONE {
                for i in 0..n {
                    self.bytes(bones + i * 0x20 + 0x1C, 4);
                }
            }
        }
    }

    fn is_skeleton(&self, sk: u32) -> bool {
        if sk == NONE || !self.inb(sk, 0x14) {
            return false;
        }
        let n = self.word(sk);
        let bones = self.target(sk + 0x10);
        if n == 0 || n > 0x100 || bones == NONE || !self.inb(bones, n * 0x20) {
            return false;
        }
        let mut ch = 0u32;
        for i in 0..n {
            let b = bones + i * 0x20;
            let l: f32 = (0..4).map(|k| self.f32(b + 4 * k).powi(2)).sum();
            if !(0.9..1.1).contains(&l) {
                return false;
            }
            ch += self.byte(b + 0x1C) as u32;
        }
        ch == n - 1
    }

    fn is_motion_anim(&self, m: u32) -> bool {
        if m == NONE || !self.inb(m, 0x34) || self.word(m) > 0x100 || self.word(m + 8) > 0x400 {
            return false;
        }
        let fps = self.f32(m + 0x18);
        (1.0..=1000.0).contains(&fps) && self.is_skeleton(self.target(m + 0x14))
    }

    fn is_key_anim(&self, a: u32) -> bool {
        if a == NONE || !self.inb(a, 0x34) {
            return false;
        }
        let bones = self.word(a + 0x10);
        let frames = self.word(a + 0x28);
        let (t0, t1) = (self.f32(a + 8), self.f32(a + 0xC));
        bones != 0
            && bones <= 0x100
            && frames != 0
            && frames <= 0x10000
            && t0 > -1e7
            && t1 < 1e7
            && t0 <= t1
            && self.target(a + 0x1C) != NONE
            && self.target(a + 0x20) != NONE
            && self.target(a + 0x24) != NONE
            && self.inb(self.target(a + 0x1C), bones * 2)
    }

    // ---- interface (HUD) --------------------------------------------------
    fn obj_anim(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        for (o, k) in [(0, KEYS_VEC4), (8, KEYS_VEC4), (0x10, KEYS_VEC4), (0x18, KEYS_COLOR), (0x20, KEYS_UV)] {
            self.track(off + o, k);
        }
    }
    fn state_anim(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        for (o, k) in [(0, KEYS_ACTIVE), (8, KEYS_VEC4), (0x10, KEYS_COLOR), (0x18, KEYS_VEC4), (0x20, KEYS_UV), (0x28, KEYS_VEC4)] {
            self.track(off + o, k);
        }
        let s = self.target(off + 0x30);
        self.sound_seq(s);
        self.action_list(off + 0x34);
    }
    fn var_value(&mut self, off: u32) {
        if off != NONE {
            self.bytes(off, 2);
            self.u16(off + 2);
        }
    }
    fn action_list(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        let lst = self.target(off + 4);
        if lst == NONE {
            return;
        }
        for i in 0..self.word(off).min(0x400) {
            let a = lst + i * 8;
            let data = self.target(a + 4);
            let ty = self.byte(a);
            self.bytes(a, 4);
            if data == NONE {
                continue;
            }
            match ty {
                1 => {
                    self.bytes(data, 2);
                    self.u16(data + 2);
                }
                2 => {
                    let cv = self.target(data);
                    if cv != NONE {
                        let v = self.target(cv + 4);
                        self.var_value(v);
                    }
                    let v = self.target(data + 4);
                    self.var_value(v);
                }
                3 => self.var_value(data),
                _ => {}
            }
        }
    }
    fn event_def(&mut self, off: u32) {
        if off == NONE || self.claim_at(off + 4) == Claim::Bytes {
            return;
        }
        let ty = self.byte(off + 5);
        if ty == 1 || ty == 0x20 {
            let h = self.target(off);
            self.im_header(h);
        } else if ty == 0xB {
            let a = self.target(off);
            self.action_list(a);
        }
        self.bytes(off + 4, 2);
        self.u16(off + 6);
        let s = self.target(off + 0x14);
        self.sound_seq(s);
        let args = self.target(off + 8);
        if args != NONE {
            if ty == 0x11 {
                self.var_value(args);
            } else {
                let (a0, a1) = (self.target(args), self.target(args + 4));
                self.var_value(a0);
                self.var_value(a1);
            }
        }
        let b = self.target(off + 0x18);
        if b != NONE {
            self.bytes(b, 4);
        }
        let mods = self.target(off + 0x20);
        if mods != NONE {
            for i in 0..self.word(off + 0x1C).min(0x100) {
                self.bytes(mods + i * 8, 4);
            }
        }
    }
    fn event_list(&mut self, cnt: u32, lo: u32) {
        let lst = self.target(lo);
        if lst == NONE {
            return;
        }
        for i in 0..self.word(cnt).min(0x1000) {
            self.bytes(lst + i * 0xC, 4);
            let d = self.target(lst + i * 0xC + 8);
            self.event_def(d);
        }
    }
    fn im_object_data(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        self.bytes(off, 8);
        let st = self.target(off + 0xC);
        if st != NONE {
            for i in 0..self.word(off + 8).min(0x1000) {
                self.bytes(st + i * 0x28 + 0x14, 4);
            }
        }
        self.event_list(off + 0x10, off + 0x14);
    }
    fn font(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        self.bytes(off + 8, 4);
        let sp = self.target(off + 0x24);
        if sp != NONE {
            for i in 0..self.word(off + 0x20).min(0x200) {
                self.bytes(sp + i * 0xC, 4);
            }
        }
    }
    fn cstring(&mut self, off: u32) {
        if off == NONE {
            return;
        }
        let mut n = 0;
        while off + n < self.size() && self.b[(off + n) as usize] != 0 {
            n += 1;
        }
        self.cl(off, n + 1, Claim::Str);
    }
    fn im_object(&mut self, off: u32) {
        self.bytes(off, 10);
        self.u16(off + 0xA);
        self.u16(off + 0xC);
        self.u16(off + 0xE);
        let sub = self.target(off + 0x14);
        let ty = self.byte(off + 1);
        if sub != NONE {
            match ty {
                0 => {
                    if self.byte(sub) == 3 {
                        let an = self.target(sub + 0x10);
                        if an != NONE {
                            self.bytes(an + 0xC, 2);
                            self.u16(an + 0xE);
                        }
                    }
                    self.bytes(sub, 4);
                }
                1 | 10 | 12 => {
                    self.bytes(sub + 0xC, 8);
                    self.u16(sub + 0x14);
                    self.u16(sub + 0x16);
                    let st = self.target(sub);
                    if st != NONE {
                        let s = self.target(st);
                        self.cstring(s);
                    }
                    if self.byte(sub + 0x10) == 1 {
                        let lst = self.target(sub + 8);
                        if lst != NONE {
                            let texts = self.target(lst + 4);
                            self.bytes(lst + 8, 2);
                            self.u16(lst + 0xA);
                            if texts != NONE {
                                for k in 0..self.word(lst).min(0x400) {
                                    let t = self.target(texts + 4 * k);
                                    if t != NONE {
                                        let s = self.target(t);
                                        self.cstring(s);
                                    }
                                }
                            }
                        }
                    }
                    let f = self.target(sub + if ty == 10 { 0x10 } else { 4 });
                    self.font(f);
                }
                _ => {}
            }
        }
        let a = self.target(off + 0x60);
        self.obj_anim(a);
        let d = self.target(off + 0x44);
        self.im_object_data(d);
        if ty == 6 || ty == 7 {
            let h = self.target(off + 0x3C);
            if h != NONE {
                let hh = self.target(h);
                self.im_header(hh);
            }
        }
        let cl = self.target(off + 0x48);
        if cl != NONE {
            let c = self.target(cl + 8);
            let n = self.word(cl + 4).min(0x100);
            self.bytes(cl, 4);
            if c != NONE {
                for i in 0..n {
                    let cond = c + i * 0x1C;
                    let data = self.target(cond + 8);
                    match self.byte(cond) {
                        1 => self.var_value(data),
                        2 if data != NONE => {
                            let v = self.target(data + 4);
                            self.var_value(v);
                        }
                        _ => {}
                    }
                    self.bytes(cond, 4);
                }
            }
        }
        let st = self.target(off + 0x68);
        if st != NONE {
            for i in 0..self.word(off + 0x64).min(0x100) {
                self.bytes(st + i * 0xC, 4);
                let s = self.target(st + i * 0xC + 8);
                self.state_anim(s);
            }
        }
    }
    fn im_header(&mut self, off: u32) {
        if off == NONE || self.claim_at(off) == Claim::U16 {
            return;
        }
        self.u16(off);
        self.u16(off + 2);
        self.bytes(off + 0x3C, 4);
        let objs = self.target(off + 0x10);
        if objs != NONE {
            for i in 0..self.word(off + 0xC).min(500) {
                self.im_object(objs + i * 0x6C);
            }
        }
        self.event_list(off + 0x14, off + 0x18);
        let v = self.target(off + 0x1C);
        if v != NONE {
            let lst = self.target(v + 4);
            if lst != NONE {
                for i in 0..self.word(v).min(0x1000) {
                    let vv = lst + i * 0x34;
                    let cv = self.target(vv + 0x10);
                    self.bytes(vv, 4);
                    if cv != NONE {
                        let val = self.target(cv + 4);
                        self.var_value(val);
                    }
                }
            }
        }
        let mut i = 0x20;
        while i < 0x30 {
            let s = self.target(off + i);
            self.sound_seq(s);
            i += 4;
        }
    }

    // ---- CarAInfo ---------------------------------------------------------
    fn car_a_info(&mut self, off: u32) {
        for i in 0..4 {
            let s = self.target(off + 0x90 + 4 * i);
            self.ai_script(s);
        }
        self.bytes(off + 0x10, 4);
        self.u16(off + 0x14);
        self.u16(off + 0x16);
        let p = self.target(off + 0x6C);
        if p != NONE {
            self.bytes(p, 4);
        }
        let p = self.target(off + 0x78);
        if p != NONE {
            let m = self.target(p + 0x10);
            self.motion_anim(m);
            if m != NONE {
                let lst = self.target(m + 0x38);
                if lst != NONE {
                    let parts = self.target(lst + 4);
                    if parts != NONE {
                        for i in 0..self.word(lst).min(0x100) {
                            let c = parts + i * 0x1C;
                            self.bytes(c, 4);
                            self.u16(c + 4);
                            self.u16(c + 6);
                            let g = self.target(c + 8);
                            self.geom_root(g);
                        }
                    }
                }
            }
            for k in [0x14, 0x18, 0x1C] {
                let a = self.target(p + k);
                self.key_anim(a);
            }
        }
        let a = self.target(off + 0x48);
        self.key_anim(a);
        let g = self.target(off + 0x54);
        self.geom_root(g);
        let p = self.target(off + 0x80);
        if p != NONE {
            self.camera_list(p + 0x80);
            self.camera_list(p + 0x88);
        }
        let p = self.target(off + 0x84);
        if p != NONE {
            self.bytes(p + 0x34, 4);
            for k in [0x10, 0x18, 0x1C] {
                let h = self.target(p + k);
                self.im_header(h);
            }
        }
        let st = self.target(off + 0x8C);
        if st != NONE {
            let t = self.target(st + 0x40);
            if t != NONE {
                for i in 0..self.word(st + 0x3C).min(0x40) {
                    let t2 = self.target(t + 4 * i);
                    if t2 != NONE {
                        let s = self.target(t2);
                        self.strs.insert(s);
                    }
                }
            }
            let t = self.target(st + 0x48);
            if t != NONE {
                let s = self.target(t);
                self.strs.insert(s);
            }
        }
        let p = self.target(off + 0xA0);
        if p != NONE {
            self.bytes(p + 0x10, 4);
        }
        let p = self.target(off + 0xA4);
        if p != NONE {
            let pts = self.target(p + 8);
            if pts != NONE {
                for i in 0..self.word(p + 4).min(0x100) {
                    self.bytes(pts + i * 0x30 + 0x10, 4);
                }
            }
        }
        let p = self.target(off + 0xE4);
        if p != NONE {
            let n = self.word(off + 0xE0);
            let n = if n < 0x40 { n.max(11) } else { 11 };
            for i in 0..n {
                self.slave_list(p + i * 0xC + 4);
            }
        }
        for (cnt, ptr) in [(0xE8, 0xEC), (0xF0, 0xF4)] {
            let p = self.target(off + ptr);
            if p != NONE {
                for i in 0..self.word(off + cnt).min(0x40) {
                    self.slave_list(p + i * 12 + 4);
                }
            }
        }
        let s = self.target(off + 0x60);
        self.sound(s);
        let p = self.target(off + 0xDC);
        if p != NONE {
            let s = self.target(p + 0xC);
            self.sound(s);
        }
        let p = self.target(off + 0x11C);
        if p != NONE {
            for i in 0..self.word(off + 0x118).min(0x40) {
                let ss = self.target(p + 4 * i);
                self.sound(ss);
            }
        }
        let p = self.target(off + 0x104);
        if p != NONE {
            for i in 0..self.word(off + 0x100).min(0x40) {
                self.u16(p + i * 0x50 + 0x30);
                self.u16(p + i * 0x50 + 0x32);
            }
        }
        let p = self.target(off + 0xD4);
        if p != NONE {
            for i in 0..self.word(off + 0xD0).min(0x20) {
                let w = p + i * 0x50;
                self.bytes(w + 0x3C, 4);
                self.u16(w + 0x40);
                self.bytes(w + 0x42, 2);
            }
        }
    }

    fn string_targets(&mut self) {
        let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        for i in 0..self.pointers.len() {
            let t = self.target(self.pointers[i]);
            if t == NONE || self.claim_at(t) != Claim::Default {
                continue;
            }
            let c0 = self.b[t as usize];
            if !is_word(c0) && (c0 == 0 || !b"\\/.|[(\"'-+#*<!".contains(&c0)) {
                continue;
            }
            let (mut n, mut w) = (0u32, 0u32);
            while t + n < self.size() && (0x20..0x7F).contains(&self.b[(t + n) as usize]) {
                w += is_word(self.b[(t + n) as usize]) as u32;
                n += 1;
            }
            if n >= 3 && (w * 2 > n || n >= 8) && t + n < self.size() && self.b[(t + n) as usize] == 0 {
                self.cl(t, n + 1, Claim::Str);
            }
        }
    }
}

/// Walk a car pack blob in byte order `e`.
pub fn walk(blob: &Blob, e: Endian) -> CarWalk {
    let size = blob.data.len();
    let mut w = W {
        b: &blob.data,
        e,
        claim: vec![Claim::Default; size],
        pointers: blob.pointers.iter().copied().collect(),
        geom: BTreeSet::new(),
        strs: BTreeSet::new(),
        es_depth: 0,
        sl_depth: 0,
    };
    let set = |claim: &mut Vec<Claim>, off: u32, c: Claim| {
        let o = off as usize;
        if o + 4 <= claim.len() {
            claim[o..o + 4].fill(c);
        }
    };
    for &p in &blob.pointers {
        set(&mut w.claim, p, Claim::Ptr);
    }
    // ref_relocs slots: pointers too (local: += base; foreign: shared export)
    for r in &blob.ref_relocs {
        set(&mut w.claim, r.slot, if r.pack() == 0 { Claim::Ptr } else { Claim::RefRel });
    }
    for r in &blob.texture_relocs {
        if w.claim_at(r.slot) == Claim::Default {
            set(&mut w.claim, r.slot, Claim::TexRef);
        }
    }
    for r in &blob.sound_relocs {
        if w.claim_at(r.slot) == Claim::Default {
            set(&mut w.claim, r.slot, Claim::SndRef);
        }
    }
    // local ref relocs count as pointer slots for target()
    for r in &blob.ref_relocs {
        if r.pack() == 0 {
            w.pointers.push(r.slot);
        }
    }

    // shape scans (objects referenced from many places)
    for i in 0..w.pointers.len() {
        let m = w.target(w.pointers[i]);
        if w.is_motion_anim(m) {
            w.motion_anim(m);
        }
    }
    for i in 0..w.pointers.len() {
        let t = w.target(w.pointers[i]);
        if w.is_event_seq(t) {
            w.event_seq(t);
        }
    }
    for i in 0..w.pointers.len() {
        let a = w.target(w.pointers[i]);
        if w.is_key_anim(a) {
            w.key_anim(a);
        }
    }
    let tab = w.target(4);
    let mut car_info = None;
    if tab != NONE {
        for i in 0..w.word(0) {
            let o = w.target(tab + 8 * i);
            if o != NONE {
                car_info.get_or_insert(o);
                w.car_a_info(o);
            }
        }
    }
    for t in std::mem::take(&mut w.strs) {
        w.cstring(t);
    }
    w.string_targets();
    w.geom.remove(&NONE);

    // Turn claims into a swap plan (Port_Swap's Swapper_Finish rules).
    let mut swap = SwapMap::new(size);
    let wordish = |c: Claim| matches!(c, Claim::Ptr | Claim::U32 | Claim::TexRef | Claim::SndRef | Claim::RefRel);
    let full = size - size % 4;
    for i in (0..full).step_by(4) {
        let q = &w.claim[i..i + 4];
        if q.iter().all(|&c| c == Claim::Default) || wordish(q[0]) {
            swap.claim(i as u32, 4, Kind::Word);
            continue;
        }
        for h in [0, 2] {
            let (a, b) = (q[h], q[h + 1]);
            if a == Claim::U16 || (a == Claim::Default && b == Claim::Default) {
                swap.claim((i + h) as u32, 2, Kind::Half);
            } else {
                swap.claim((i + h) as u32, 2, Kind::Byte);
            }
        }
    }
    swap.claim(full as u32, (size - full) as u32, Kind::Byte);

    let geometry_roots = w.geom.clone();
    let _ = HashMap::<u32, u32>::new;
    CarWalk { swap, geometry_roots, car_info }
}
