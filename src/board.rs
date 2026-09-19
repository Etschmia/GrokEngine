//! Mailbox board, FEN, make/unmake, attacks, and legal move generation.
//!
//! Square 0 is a1, 7 is h1, 56 is a8, 63 is h8.

use std::fmt;

pub const EMPTY: u8 = 0;
pub const PAWN: u8 = 1;
pub const KNIGHT: u8 = 2;
pub const BISHOP: u8 = 3;
pub const ROOK: u8 = 4;
pub const QUEEN: u8 = 5;
pub const KING: u8 = 6;

const WK: u8 = 1;
const WQ: u8 = 2;
const BK: u8 = 4;
const BQ: u8 = 8;

const FLAG_EP: u8 = 1;
const FLAG_CASTLE: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[inline]
    pub fn flip(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    #[inline]
    pub fn idx(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub promo: u8,
    pub flags: u8,
}

impl Move {
    #[inline]
    pub fn new(from: u8, to: u8) -> Self {
        Self {
            from,
            to,
            promo: 0,
            flags: 0,
        }
    }

    #[inline]
    pub fn promo(from: u8, to: u8, promo: u8) -> Self {
        Self {
            from,
            to,
            promo,
            flags: 0,
        }
    }

    #[inline]
    pub fn is_ep(self) -> bool {
        self.flags & FLAG_EP != 0
    }

    #[inline]
    pub fn is_castle(self) -> bool {
        self.flags & FLAG_CASTLE != 0
    }

    pub fn to_lan(self) -> String {
        let mut s = String::with_capacity(5);
        s.push(file_char(self.from));
        s.push(rank_char(self.from));
        s.push(file_char(self.to));
        s.push(rank_char(self.to));
        match self.promo {
            KNIGHT => s.push('n'),
            BISHOP => s.push('b'),
            ROOK => s.push('r'),
            QUEEN => s.push('q'),
            _ => {}
        }
        s
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_lan())
    }
}

#[inline]
fn file_of(sq: u8) -> u8 {
    sq & 7
}

#[inline]
fn rank_of(sq: u8) -> u8 {
    sq >> 3
}

#[inline]
fn file_char(sq: u8) -> char {
    (b'a' + file_of(sq)) as char
}

#[inline]
fn rank_char(sq: u8) -> char {
    (b'1' + rank_of(sq)) as char
}

#[inline]
fn sq_of(file: u8, rank: u8) -> u8 {
    rank * 8 + file
}

#[inline]
fn dest(sq: u8, dfile: i8, drank: i8) -> Option<u8> {
    let f = file_of(sq) as i8 + dfile;
    let r = rank_of(sq) as i8 + drank;
    if (0..8).contains(&f) && (0..8).contains(&r) {
        Some(sq_of(f as u8, r as u8))
    } else {
        None
    }
}

#[inline]
pub fn color_of(piece: u8) -> Color {
    if piece & 8 == 0 {
        Color::White
    } else {
        Color::Black
    }
}

#[inline]
pub fn type_of(piece: u8) -> u8 {
    piece & 7
}

#[inline]
fn make_piece(color: Color, pt: u8) -> u8 {
    if color == Color::White {
        pt
    } else {
        pt | 8
    }
}

const KNIGHT_D: [(i8, i8); 8] = [
    (1, 2),
    (1, -2),
    (-1, 2),
    (-1, -2),
    (2, 1),
    (2, -1),
    (-2, 1),
    (-2, -1),
];
const KING_D: [(i8, i8); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];
const BISHOP_D: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const ROOK_D: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

#[derive(Clone, Copy)]
pub struct Undo {
    captured: u8,
    cap_sq: u8,
    castling: u8,
    ep: Option<u8>,
    halfmove: u16,
    fullmove: u16,
    king: [u8; 2],
    hash: u64,
}

#[derive(Clone, Copy)]
pub struct NullUndo {
    ep: Option<u8>,
    hash: u64,
}

#[derive(Clone)]
pub struct Position {
    pub squares: [u8; 64],
    pub side: Color,
    pub castling: u8,
    pub ep: Option<u8>,
    pub halfmove: u16,
    pub fullmove: u16,
    pub king: [u8; 2],
    pub hash: u64,
}

impl Position {
    pub fn empty() -> Self {
        Self {
            squares: [EMPTY; 64],
            side: Color::White,
            castling: 0,
            ep: None,
            halfmove: 0,
            fullmove: 1,
            king: [0, 0],
            hash: 0,
        }
    }

    pub fn startpos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .expect("startpos fen")
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let mut parts = fen.split_whitespace();
        let board = parts.next().ok_or("missing board")?;
        let side = parts.next().ok_or("missing side")?;
        let castle = parts.next().unwrap_or("-");
        let ep = parts.next().unwrap_or("-");
        let half = parts.next().unwrap_or("0");
        let full = parts.next().unwrap_or("1");

        let mut pos = Position::empty();
        let mut sq_rank = 7i8;
        let mut sq_file = 0i8;
        for c in board.chars() {
            match c {
                '/' => {
                    if sq_file != 8 {
                        return Err("rank length".into());
                    }
                    sq_file = 0;
                    sq_rank -= 1;
                    if sq_rank < 0 {
                        return Err("too many ranks".into());
                    }
                }
                '1'..='8' => {
                    sq_file += (c as u8 - b'0') as i8;
                    if sq_file > 8 {
                        return Err("rank overflow".into());
                    }
                }
                _ => {
                    let (color, pt) = match c {
                        'P' => (Color::White, PAWN),
                        'N' => (Color::White, KNIGHT),
                        'B' => (Color::White, BISHOP),
                        'R' => (Color::White, ROOK),
                        'Q' => (Color::White, QUEEN),
                        'K' => (Color::White, KING),
                        'p' => (Color::Black, PAWN),
                        'n' => (Color::Black, KNIGHT),
                        'b' => (Color::Black, BISHOP),
                        'r' => (Color::Black, ROOK),
                        'q' => (Color::Black, QUEEN),
                        'k' => (Color::Black, KING),
                        _ => return Err(format!("bad piece {c}")),
                    };
                    if sq_file >= 8 || sq_rank < 0 {
                        return Err("piece off board".into());
                    }
                    let sq = sq_of(sq_file as u8, sq_rank as u8);
                    let piece = make_piece(color, pt);
                    pos.squares[sq as usize] = piece;
                    if pt == KING {
                        pos.king[color.idx()] = sq;
                    }
                    sq_file += 1;
                }
            }
        }
        if sq_rank != 0 || sq_file != 8 {
            return Err("incomplete board".into());
        }

        pos.side = match side {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err("bad side".into()),
        };

        pos.castling = 0;
        if castle != "-" {
            for c in castle.chars() {
                match c {
                    'K' => pos.castling |= WK,
                    'Q' => pos.castling |= WQ,
                    'k' => pos.castling |= BK,
                    'q' => pos.castling |= BQ,
                    '-' => {}
                    _ => return Err("bad castling".into()),
                }
            }
        }

        pos.ep = if ep == "-" {
            None
        } else {
            Some(parse_sq(ep).ok_or("bad ep")?)
        };
        pos.halfmove = half.parse().unwrap_or(0);
        pos.fullmove = full.parse().unwrap_or(1);
        pos.hash = crate::zobrist::hash_pos(&pos);
        Ok(pos)
    }

    pub fn to_fen(&self) -> String {
        let mut s = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let p = self.squares[sq_of(file, rank) as usize];
                if p == EMPTY {
                    empty += 1;
                } else {
                    if empty > 0 {
                        s.push((b'0' + empty) as char);
                        empty = 0;
                    }
                    s.push(piece_char(p));
                }
            }
            if empty > 0 {
                s.push((b'0' + empty) as char);
            }
            if rank > 0 {
                s.push('/');
            }
        }
        s.push(' ');
        s.push(if self.side == Color::White { 'w' } else { 'b' });
        s.push(' ');
        if self.castling == 0 {
            s.push('-');
        } else {
            if self.castling & WK != 0 {
                s.push('K');
            }
            if self.castling & WQ != 0 {
                s.push('Q');
            }
            if self.castling & BK != 0 {
                s.push('k');
            }
            if self.castling & BQ != 0 {
                s.push('q');
            }
        }
        s.push(' ');
        match self.ep {
            None => s.push('-'),
            Some(sq) => {
                s.push(file_char(sq));
                s.push(rank_char(sq));
            }
        }
        s.push_str(&format!(" {} {}", self.halfmove, self.fullmove));
        s
    }

    #[inline]
    pub fn piece_at(&self, sq: u8) -> u8 {
        self.squares[sq as usize]
    }

    pub fn in_check(&self) -> bool {
        self.attacked_by(self.king[self.side.idx()], self.side.flip())
    }

    pub fn attacked_by(&self, sq: u8, by: Color) -> bool {
        let pawn = make_piece(by, PAWN);
        let pawn_dir: i8 = if by == Color::White { -1 } else { 1 };
        if let Some(s) = dest(sq, 1, pawn_dir) {
            if self.squares[s as usize] == pawn {
                return true;
            }
        }
        if let Some(s) = dest(sq, -1, pawn_dir) {
            if self.squares[s as usize] == pawn {
                return true;
            }
        }

        let knight = make_piece(by, KNIGHT);
        for &(df, dr) in &KNIGHT_D {
            if let Some(s) = dest(sq, df, dr) {
                if self.squares[s as usize] == knight {
                    return true;
                }
            }
        }

        let king = make_piece(by, KING);
        for &(df, dr) in &KING_D {
            if let Some(s) = dest(sq, df, dr) {
                if self.squares[s as usize] == king {
                    return true;
                }
            }
        }

        let bishop = make_piece(by, BISHOP);
        let queen = make_piece(by, QUEEN);
        let rook = make_piece(by, ROOK);

        for &(df, dr) in &BISHOP_D {
            let mut f = df;
            let mut r = dr;
            while let Some(s) = dest(sq, f, r) {
                let p = self.squares[s as usize];
                if p != EMPTY {
                    if p == bishop || p == queen {
                        return true;
                    }
                    break;
                }
                f += df;
                r += dr;
            }
        }
        for &(df, dr) in &ROOK_D {
            let mut f = df;
            let mut r = dr;
            while let Some(s) = dest(sq, f, r) {
                let p = self.squares[s as usize];
                if p != EMPTY {
                    if p == rook || p == queen {
                        return true;
                    }
                    break;
                }
                f += df;
                r += dr;
            }
        }
        false
    }

    pub fn make(&mut self, m: Move) -> Undo {
        let from = m.from as usize;
        let to = m.to as usize;
        let piece = self.squares[from];
        let pt = type_of(piece);
        let us = self.side;
        let them = us.flip();

        let undo = Undo {
            captured: EMPTY,
            cap_sq: m.to,
            castling: self.castling,
            ep: self.ep,
            halfmove: self.halfmove,
            fullmove: self.fullmove,
            king: self.king,
            hash: self.hash,
        };

        self.hash ^= crate::zobrist::piece_key(us, pt, m.from);
        self.squares[from] = EMPTY;

        let mut captured = self.squares[to];
        let mut cap_sq = m.to;

        if m.is_ep() {
            let pawn_sq = if us == Color::White {
                m.to - 8
            } else {
                m.to + 8
            };
            captured = self.squares[pawn_sq as usize];
            cap_sq = pawn_sq;
            self.squares[pawn_sq as usize] = EMPTY;
            self.hash ^= crate::zobrist::piece_key(them, PAWN, pawn_sq);
        } else if captured != EMPTY {
            self.hash ^= crate::zobrist::piece_key(color_of(captured), type_of(captured), m.to);
        }

        let new_pt = if m.promo != 0 { m.promo } else { pt };
        self.squares[to] = make_piece(us, new_pt);
        self.hash ^= crate::zobrist::piece_key(us, new_pt, m.to);

        if m.is_castle() {
            if m.to == m.from + 2 {
                let rfrom = m.from + 3;
                let rto = m.from + 1;
                self.squares[rto as usize] = self.squares[rfrom as usize];
                self.squares[rfrom as usize] = EMPTY;
                self.hash ^= crate::zobrist::piece_key(us, ROOK, rfrom);
                self.hash ^= crate::zobrist::piece_key(us, ROOK, rto);
            } else {
                let rfrom = m.from - 4;
                let rto = m.from - 1;
                self.squares[rto as usize] = self.squares[rfrom as usize];
                self.squares[rfrom as usize] = EMPTY;
                self.hash ^= crate::zobrist::piece_key(us, ROOK, rfrom);
                self.hash ^= crate::zobrist::piece_key(us, ROOK, rto);
            }
        }

        if pt == KING {
            self.king[us.idx()] = m.to;
        }

        self.hash ^= crate::zobrist::castle_key(self.castling);
        let mut cr = self.castling;
        cr &= castle_mask(m.from);
        cr &= castle_mask(m.to);
        self.castling = cr;
        self.hash ^= crate::zobrist::castle_key(self.castling);

        if let Some(ep) = self.ep {
            self.hash ^= crate::zobrist::ep_key(file_of(ep));
        }
        self.ep = None;
        if pt == PAWN && rank_of(m.from).abs_diff(rank_of(m.to)) == 2 {
            let ep = sq_of(file_of(m.from), (rank_of(m.from) + rank_of(m.to)) / 2);
            self.ep = Some(ep);
            self.hash ^= crate::zobrist::ep_key(file_of(ep));
        }

        if pt == PAWN || captured != EMPTY {
            self.halfmove = 0;
        } else {
            self.halfmove = self.halfmove.saturating_add(1);
        }

        if us == Color::Black {
            self.fullmove = self.fullmove.saturating_add(1);
        }
        self.side = us.flip();
        self.hash ^= crate::zobrist::side_key();

        Undo {
            captured,
            cap_sq,
            ..undo
        }
    }

    pub fn unmake(&mut self, m: Move, u: Undo) {
        self.side = self.side.flip();
        self.castling = u.castling;
        self.ep = u.ep;
        self.halfmove = u.halfmove;
        self.fullmove = u.fullmove;
        self.king = u.king;
        self.hash = u.hash;

        let us = self.side;
        let from = m.from as usize;
        let to = m.to as usize;

        let moving = if m.promo != 0 {
            make_piece(us, PAWN)
        } else {
            self.squares[to]
        };

        if m.is_castle() {
            self.squares[from] = moving;
            self.squares[to] = EMPTY;
            if m.to == m.from + 2 {
                let rfrom = (m.from + 3) as usize;
                let rto = (m.from + 1) as usize;
                self.squares[rfrom] = self.squares[rto];
                self.squares[rto] = EMPTY;
            } else {
                let rfrom = (m.from - 4) as usize;
                let rto = (m.from - 1) as usize;
                self.squares[rfrom] = self.squares[rto];
                self.squares[rto] = EMPTY;
            }
        } else if m.is_ep() {
            self.squares[from] = moving;
            self.squares[to] = EMPTY;
            self.squares[u.cap_sq as usize] = u.captured;
        } else {
            self.squares[from] = moving;
            self.squares[to] = u.captured;
        }
    }

    pub fn make_null(&mut self) -> NullUndo {
        let undo = NullUndo {
            ep: self.ep,
            hash: self.hash,
        };
        if let Some(ep) = self.ep {
            self.hash ^= crate::zobrist::ep_key(file_of(ep));
        }
        self.ep = None;
        self.side = self.side.flip();
        self.hash ^= crate::zobrist::side_key();
        undo
    }

    pub fn unmake_null(&mut self, u: NullUndo) {
        self.side = self.side.flip();
        self.ep = u.ep;
        self.hash = u.hash;
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let mut pos = self.clone();
        let mut moves = Vec::with_capacity(64);
        pos.gen_legal_into(&mut moves);
        moves
    }

    /// Generate legal moves into `moves` (cleared first). Uses make/unmake on self.
    pub fn gen_legal_into(&mut self, moves: &mut Vec<Move>) {
        moves.clear();
        self.gen_pseudo(moves);
        let mut i = 0;
        while i < moves.len() {
            let m = moves[i];
            let u = self.make(m);
            let king = self.king[self.side.flip().idx()];
            let legal = !self.attacked_by(king, self.side);
            self.unmake(m, u);
            if legal {
                i += 1;
            } else {
                moves.swap_remove(i);
            }
        }
    }

    /// Legal captures and promotions, for quiescence search.
    pub fn gen_captures_into(&mut self, moves: &mut Vec<Move>) {
        moves.clear();
        self.gen_captures_pseudo(moves);
        let mut i = 0;
        while i < moves.len() {
            let m = moves[i];
            let u = self.make(m);
            let king = self.king[self.side.flip().idx()];
            let legal = !self.attacked_by(king, self.side);
            self.unmake(m, u);
            if legal {
                i += 1;
            } else {
                moves.swap_remove(i);
            }
        }
    }

    pub fn has_non_pawn_material(&self, color: Color) -> bool {
        for sq in 0..64 {
            let p = self.squares[sq];
            if p != EMPTY && color_of(p) == color {
                let t = type_of(p);
                if t != PAWN && t != KING {
                    return true;
                }
            }
        }
        false
    }

    pub fn move_from_lan(&self, lan: &str) -> Option<Move> {
        let lan = lan.trim();
        self.legal_moves().into_iter().find(|m| m.to_lan() == lan)
    }

    fn gen_pseudo(&self, moves: &mut Vec<Move>) {
        let us = self.side;
        for sq in 0..64u8 {
            let p = self.squares[sq as usize];
            if p == EMPTY || color_of(p) != us {
                continue;
            }
            match type_of(p) {
                PAWN => self.gen_pawn(sq, moves),
                KNIGHT => self.gen_leaper(sq, &KNIGHT_D, moves),
                BISHOP => self.gen_slider(sq, &BISHOP_D, moves),
                ROOK => self.gen_slider(sq, &ROOK_D, moves),
                QUEEN => {
                    self.gen_slider(sq, &BISHOP_D, moves);
                    self.gen_slider(sq, &ROOK_D, moves);
                }
                KING => {
                    self.gen_leaper(sq, &KING_D, moves);
                    self.gen_castling(sq, moves);
                }
                _ => {}
            }
        }
    }

    fn gen_captures_pseudo(&self, moves: &mut Vec<Move>) {
        let us = self.side;
        for sq in 0..64u8 {
            let p = self.squares[sq as usize];
            if p == EMPTY || color_of(p) != us {
                continue;
            }
            match type_of(p) {
                PAWN => self.gen_pawn_noisy(sq, moves),
                KNIGHT => self.gen_leaper_caps(sq, &KNIGHT_D, moves),
                BISHOP => self.gen_slider_caps(sq, &BISHOP_D, moves),
                ROOK => self.gen_slider_caps(sq, &ROOK_D, moves),
                QUEEN => {
                    self.gen_slider_caps(sq, &BISHOP_D, moves);
                    self.gen_slider_caps(sq, &ROOK_D, moves);
                }
                KING => self.gen_leaper_caps(sq, &KING_D, moves),
                _ => {}
            }
        }
    }

    fn gen_pawn(&self, sq: u8, moves: &mut Vec<Move>) {
        let us = self.side;
        let dir: i8 = if us == Color::White { 1 } else { -1 };
        let promo_rank = if us == Color::White { 7 } else { 0 };
        let start_rank = if us == Color::White { 1 } else { 6 };

        if let Some(to) = dest(sq, 0, dir) {
            if self.squares[to as usize] == EMPTY {
                if rank_of(to) == promo_rank {
                    push_promos(sq, to, moves);
                } else {
                    moves.push(Move::new(sq, to));
                    if rank_of(sq) == start_rank {
                        if let Some(to2) = dest(sq, 0, dir * 2) {
                            if self.squares[to2 as usize] == EMPTY {
                                moves.push(Move::new(sq, to2));
                            }
                        }
                    }
                }
            }
        }

        for df in [-1i8, 1] {
            if let Some(to) = dest(sq, df, dir) {
                let target = self.squares[to as usize];
                if target != EMPTY && color_of(target) != us {
                    if rank_of(to) == promo_rank {
                        push_promos(sq, to, moves);
                    } else {
                        moves.push(Move::new(sq, to));
                    }
                } else if target == EMPTY && self.ep == Some(to) {
                    let mut m = Move::new(sq, to);
                    m.flags = FLAG_EP;
                    moves.push(m);
                }
            }
        }
    }

    fn gen_pawn_noisy(&self, sq: u8, moves: &mut Vec<Move>) {
        let us = self.side;
        let dir: i8 = if us == Color::White { 1 } else { -1 };
        let promo_rank = if us == Color::White { 7 } else { 0 };

        if let Some(to) = dest(sq, 0, dir) {
            if self.squares[to as usize] == EMPTY && rank_of(to) == promo_rank {
                push_promos(sq, to, moves);
            }
        }

        for df in [-1i8, 1] {
            if let Some(to) = dest(sq, df, dir) {
                let target = self.squares[to as usize];
                if target != EMPTY && color_of(target) != us {
                    if rank_of(to) == promo_rank {
                        push_promos(sq, to, moves);
                    } else {
                        moves.push(Move::new(sq, to));
                    }
                } else if target == EMPTY && self.ep == Some(to) {
                    let mut m = Move::new(sq, to);
                    m.flags = FLAG_EP;
                    moves.push(m);
                }
            }
        }
    }

    fn gen_leaper(&self, sq: u8, deltas: &[(i8, i8)], moves: &mut Vec<Move>) {
        let us = self.side;
        for &(df, dr) in deltas {
            if let Some(to) = dest(sq, df, dr) {
                let t = self.squares[to as usize];
                if t == EMPTY || color_of(t) != us {
                    moves.push(Move::new(sq, to));
                }
            }
        }
    }

    fn gen_leaper_caps(&self, sq: u8, deltas: &[(i8, i8)], moves: &mut Vec<Move>) {
        let us = self.side;
        for &(df, dr) in deltas {
            if let Some(to) = dest(sq, df, dr) {
                let t = self.squares[to as usize];
                if t != EMPTY && color_of(t) != us {
                    moves.push(Move::new(sq, to));
                }
            }
        }
    }

    fn gen_slider(&self, sq: u8, dirs: &[(i8, i8)], moves: &mut Vec<Move>) {
        let us = self.side;
        for &(df, dr) in dirs {
            let mut f = df;
            let mut r = dr;
            while let Some(to) = dest(sq, f, r) {
                let t = self.squares[to as usize];
                if t == EMPTY {
                    moves.push(Move::new(sq, to));
                } else {
                    if color_of(t) != us {
                        moves.push(Move::new(sq, to));
                    }
                    break;
                }
                f += df;
                r += dr;
            }
        }
    }

    fn gen_slider_caps(&self, sq: u8, dirs: &[(i8, i8)], moves: &mut Vec<Move>) {
        let us = self.side;
        for &(df, dr) in dirs {
            let mut f = df;
            let mut r = dr;
            while let Some(to) = dest(sq, f, r) {
                let t = self.squares[to as usize];
                if t == EMPTY {
                    f += df;
                    r += dr;
                    continue;
                }
                if color_of(t) != us {
                    moves.push(Move::new(sq, to));
                }
                break;
            }
        }
    }

    fn gen_castling(&self, sq: u8, moves: &mut Vec<Move>) {
        let us = self.side;
        if self.attacked_by(sq, us.flip()) {
            return;
        }
        if us == Color::White && sq == 4 {
            if self.castling & WK != 0 {
                self.try_castle(sq, 6, &[5, 6], 7, WR_PIECE, moves);
            }
            if self.castling & WQ != 0 {
                self.try_castle(sq, 2, &[3, 2], 0, WR_PIECE, moves);
            }
        } else if us == Color::Black && sq == 60 {
            if self.castling & BK != 0 {
                self.try_castle(sq, 62, &[61, 62], 63, BR_PIECE, moves);
            }
            if self.castling & BQ != 0 {
                self.try_castle(sq, 58, &[59, 58], 56, BR_PIECE, moves);
            }
        }
    }

    fn try_castle(&self, king: u8, to: u8, through: &[u8], rook_sq: u8, rook: u8, moves: &mut Vec<Move>) {
        if self.squares[rook_sq as usize] != rook {
            return;
        }
        let (lo, hi) = if king < rook_sq {
            (king + 1, rook_sq)
        } else {
            (rook_sq + 1, king)
        };
        for sq in lo..hi {
            if self.squares[sq as usize] != EMPTY {
                return;
            }
        }
        let enemy = self.side.flip();
        for &sq in through {
            if self.attacked_by(sq, enemy) {
                return;
            }
        }
        let mut m = Move::new(king, to);
        m.flags = FLAG_CASTLE;
        moves.push(m);
    }

    pub fn perft(&mut self, depth: u32) -> u64 {
        let moves = self.legal_moves();
        if depth <= 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0;
        for m in moves {
            let u = self.make(m);
            nodes += self.perft(depth - 1);
            self.unmake(m, u);
        }
        nodes
    }

    pub fn is_capture(&self, m: Move) -> bool {
        m.is_ep() || self.squares[m.to as usize] != EMPTY
    }
}

const WR_PIECE: u8 = ROOK;
const BR_PIECE: u8 = ROOK | 8;

fn push_promos(from: u8, to: u8, moves: &mut Vec<Move>) {
    for pt in [QUEEN, ROOK, BISHOP, KNIGHT] {
        moves.push(Move::promo(from, to, pt));
    }
}

fn castle_mask(sq: u8) -> u8 {
    match sq {
        0 => !WQ,
        4 => !(WK | WQ),
        7 => !WK,
        56 => !BQ,
        60 => !(BK | BQ),
        63 => !BK,
        _ => 0xFF,
    }
}

fn parse_sq(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    if b.len() != 2 {
        return None;
    }
    let file = b[0];
    let rank = b[1];
    if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
        return None;
    }
    Some(sq_of(file - b'a', rank - b'1'))
}

fn piece_char(p: u8) -> char {
    let c = match type_of(p) {
        PAWN => 'p',
        KNIGHT => 'n',
        BISHOP => 'b',
        ROOK => 'r',
        QUEEN => 'q',
        KING => 'k',
        _ => '?',
    };
    if color_of(p) == Color::White {
        c.to_ascii_uppercase()
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_fen_roundtrip() {
        let pos = Position::startpos();
        assert_eq!(
            pos.to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
    }

    #[test]
    fn startpos_legal_count() {
        let pos = Position::startpos();
        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 20);
        let lans: Vec<String> = moves.iter().map(|m| m.to_lan()).collect();
        assert!(lans.contains(&"e2e4".into()));
        assert!(lans.contains(&"g1f3".into()));
    }

    #[test]
    fn perft_startpos_d1_d4() {
        let mut pos = Position::startpos();
        assert_eq!(pos.perft(1), 20);
        assert_eq!(pos.perft(2), 400);
        assert_eq!(pos.perft(3), 8902);
        assert_eq!(pos.perft(4), 197281);
    }

    #[test]
    fn perft_kiwipete() {
        let mut pos = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(pos.perft(1), 48);
        assert_eq!(pos.perft(2), 2039);
        assert_eq!(pos.perft(3), 97862);
    }

    #[test]
    fn perft_position3_ep_pins() {
        let mut pos =
            Position::from_fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1").unwrap();
        assert_eq!(pos.perft(1), 14);
        assert_eq!(pos.perft(2), 191);
        assert_eq!(pos.perft(3), 2812);
        assert_eq!(pos.perft(4), 43238);
    }

    #[test]
    fn perft_talkchess_promotion() {
        let mut pos = Position::from_fen(
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        )
        .unwrap();
        assert_eq!(pos.perft(1), 44);
        assert_eq!(pos.perft(2), 1486);
        assert_eq!(pos.perft(3), 62379);
    }

    #[test]
    fn apply_lan_e2e4() {
        let mut pos = Position::startpos();
        let m = pos.move_from_lan("e2e4").unwrap();
        pos.make(m);
        assert_eq!(pos.side, Color::Black);
        assert_eq!(type_of(pos.piece_at(28)), PAWN);
        assert_eq!(pos.piece_at(12), EMPTY);
        assert_eq!(pos.ep, Some(20));
    }

    #[test]
    fn castle_lan_is_king_move() {
        let pos = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        let m = pos.move_from_lan("e1g1").unwrap();
        assert!(m.is_castle());
        assert_eq!(m.to_lan(), "e1g1");
    }

    #[test]
    fn hash_updates_and_restores() {
        let mut pos = Position::startpos();
        let h0 = pos.hash;
        assert_eq!(h0, crate::zobrist::hash_pos(&pos));
        let m = pos.move_from_lan("e2e4").unwrap();
        let u = pos.make(m);
        assert_ne!(pos.hash, h0);
        assert_eq!(pos.hash, crate::zobrist::hash_pos(&pos));
        pos.unmake(m, u);
        assert_eq!(pos.hash, h0);
    }

    #[test]
    fn hash_includes_side_to_move() {
        let w = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        let b = Position::from_fen("4k3/8/8/8/8/8/8/4K3 b - - 0 1").unwrap();
        assert_ne!(w.hash, b.hash);
    }

    #[test]
    fn legal_moves_do_not_leave_king_in_check() {
        let fens = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        ];
        for fen in fens {
            let pos = Position::from_fen(fen).unwrap();
            for m in pos.legal_moves() {
                let mut p = pos.clone();
                p.make(m);
                p.side = p.side.flip();
                assert!(
                    !p.in_check(),
                    "move {} left king in check in {fen}",
                    m.to_lan()
                );
            }
        }
    }
}
