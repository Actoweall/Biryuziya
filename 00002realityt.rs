// ═══════════════════════════════════════════════════════════════════════════════════════════════
//  РЕАЛЬНОСТЬ / REALITY — песочница-симулятор реальности: от клеток Конвея до разрушаемого мира,
//  животных, людей и космоса. Один файл, чистый Rust (edition 2024), без крейтов. Windows (Win32 + GDI).
//  Сборка:  rustc -C opt-level=3 --edition 2024 reality.rs      Запуск: reality.exe
//  Разделы: PLATFORM · INPUT · FONT · UI · CONFIG · MATERIALS · WORLD · GEN · SIM · EDITOR · RENDER · GAME · MAIN
//  Правило для итераций: каждый раздел общается с другими только через свои структуры/функции.
//  Мир: 2 вокселя на метр (воксель 0.5 м); рендер — полосы кадра на все ядра; пустота пропускается блоками 4³.
// ═══════════════════════════════════════════════════════════════════════════════════════════════
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// ─── PLATFORM: окно на весь монитор, очередь событий, вывод кадра (Win32/GDI) ───────────────────
#[cfg(windows)]
mod win {
    use super::Ev;
    use std::{cell::RefCell, ffi::c_void};
    type Proc = extern "system" fn(isize, u32, usize, isize) -> isize;
    #[repr(C)] struct WndClass { style: u32, proc_: Proc, cx: i32, wx: i32, inst: isize, icon: isize, cur: isize, bg: isize, menu: *const u16, name: *const u16 }
    #[repr(C)] struct Msg { h: isize, msg: u32, wp: usize, lp: isize, time: u32, x: i32, y: i32, pv: u32 }
    #[repr(C)] struct Bmi { size: u32, w: i32, h: i32, planes: u16, bits: u16, comp: u32, img: u32, xp: i32, yp: i32, used: u32, imp: u32, pal: u32 }
    #[repr(C)] struct Pt { x: i32, y: i32 }
    #[link(name = "user32")] unsafe extern "system" {
        fn RegisterClassW(c: *const WndClass) -> u16;
        fn CreateWindowExW(ex: u32, c: *const u16, n: *const u16, s: u32, x: i32, y: i32, w: i32, h: i32, p: isize, m: isize, i: isize, l: *mut c_void) -> isize;
        fn DefWindowProcW(h: isize, m: u32, w: usize, l: isize) -> isize;
        fn PeekMessageW(m: *mut Msg, h: isize, a: u32, b: u32, r: u32) -> i32;
        fn TranslateMessage(m: *const Msg) -> i32;
        fn DispatchMessageW(m: *const Msg) -> isize;
        fn PostQuitMessage(c: i32);
        fn GetSystemMetrics(i: i32) -> i32;
        fn LoadCursorW(i: isize, n: isize) -> isize;
        fn GetDC(h: isize) -> isize;
        fn DestroyWindow(h: isize) -> i32;
        fn SetProcessDPIAware() -> i32;
        fn GetCursorPos(p: *mut Pt) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn ShowCursor(s: i32) -> i32;
    }
    #[link(name = "gdi32")] unsafe extern "system" {
        fn StretchDIBits(dc: isize, x: i32, y: i32, w: i32, h: i32, sx: i32, sy: i32, sw: i32, sh: i32, b: *const c_void, i: *const Bmi, u: u32, r: u32) -> i32;
        fn SetStretchBltMode(dc: isize, m: i32) -> i32;
    }
    #[link(name = "kernel32")] unsafe extern "system" { fn GetModuleHandleW(n: *const u16) -> isize; }
    #[link(name = "winmm")] unsafe extern "system" { fn timeBeginPeriod(p: u32) -> u32; }
    thread_local! { static EVQ: RefCell<Vec<Ev>> = const { RefCell::new(Vec::new()) }; }
    fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain([0]).collect() }
    extern "system" fn wndproc(h: isize, m: u32, w: usize, l: isize) -> isize {
        let (lo, hi) = ((l & 0xFFFF) as i32, ((l >> 16) & 0xFFFF) as i32);
        let ev = match m {
            0x0002 => { unsafe { PostQuitMessage(0) }; return 0 }
            0x0005 => Ev::Size(lo, hi),
            0x0008 => Ev::Blur,
            0x0014 => return 1,
            0x0100 | 0x0101 => Ev::Key(w as u8, m == 0x0100),
            0x0102 => match char::from_u32(w as u32) { Some(c) => Ev::Char(c), None => return 0 },
            0x0200 => Ev::Mouse(lo as i16 as i32, hi as i16 as i32),
            0x0201 | 0x0202 => Ev::Btn(0, m == 0x0201),
            0x0204 | 0x0205 => Ev::Btn(1, m == 0x0204),
            0x020A => Ev::Wheel(((w >> 16) as u16 as i16).signum() as i32),
            _ => return unsafe { DefWindowProcW(h, m, w, l) },
        };
        EVQ.with(|q| q.borrow_mut().push(ev));
        0
    }
    pub struct Window { h: isize, dc: isize, pub w: i32, pub hgt: i32, looking: bool }
    impl Window {
        pub fn new(title: &str) -> Window {
            let (cls, t) = (wide("RealitySimWnd"), wide(title));
            unsafe {
                SetProcessDPIAware();
                timeBeginPeriod(1);
                let inst = GetModuleHandleW(std::ptr::null());
                let wc = WndClass { style: 0x23, proc_: wndproc, cx: 0, wx: 0, inst, icon: 0, cur: LoadCursorW(0, 32512), bg: 0, menu: std::ptr::null(), name: cls.as_ptr() };
                RegisterClassW(&wc);
                let (w, hgt) = (GetSystemMetrics(0), GetSystemMetrics(1));
                let h = CreateWindowExW(0, cls.as_ptr(), t.as_ptr(), 0x9000_0000, 0, 0, w, hgt, 0, 0, inst, std::ptr::null_mut());
                let dc = GetDC(h);
                SetStretchBltMode(dc, 3);
                Window { h, dc, w, hgt, looking: false }
            }
        }
        pub fn pump(&mut self, out: &mut Vec<Ev>) -> bool {
            unsafe {
                let mut m: Msg = std::mem::zeroed();
                while PeekMessageW(&mut m, 0, 0, 0, 1) != 0 {
                    if m.msg == 0x0012 { return false }
                    TranslateMessage(&m);
                    DispatchMessageW(&m);
                }
            }
            EVQ.with(|q| out.append(&mut q.borrow_mut()));
            for e in out.iter() { if let Ev::Size(w, h) = *e { if w > 0 && h > 0 { (self.w, self.hgt) = (w, h) } } }
            true
        }
        /// Захват мыши для обзора: возвращает смещение курсора от центра и возвращает его в центр.
        pub fn look(&mut self, on: bool) -> (i32, i32) {
            let (cx, cy) = (self.w / 2, self.hgt / 2);
            unsafe {
                if on != self.looking { self.looking = on; ShowCursor(!on as i32); if on { SetCursorPos(cx, cy); } return (0, 0) }
                if !on { return (0, 0) }
                let mut p = Pt { x: cx, y: cy };
                GetCursorPos(&mut p);
                SetCursorPos(cx, cy);
                (p.x - cx, p.y - cy)
            }
        }
        pub fn present(&self, px: &[u32], bw: i32, bh: i32) {
            let b = Bmi { size: 40, w: bw, h: -bh, planes: 1, bits: 32, comp: 0, img: 0, xp: 0, yp: 0, used: 0, imp: 0, pal: 0 };
            unsafe { StretchDIBits(self.dc, 0, 0, self.w, self.hgt, 0, 0, bw, bh, px.as_ptr() as *const c_void, &b, 0, 0x00CC_0020); }
        }
        pub fn close(&self) { unsafe { DestroyWindow(self.h); } }
    }
}

// ─── INPUT: события ОС → состояние клавиш и мыши за кадр ────────────────────────────────────────
pub enum Ev { Key(u8, bool), Char(char), Mouse(i32, i32), Btn(u8, bool), Wheel(i32), Size(i32, i32), Blur }
pub struct Input { down: Vec<bool>, hits: Vec<u8>, chars: Vec<char>, mx: i32, my: i32, lmb: bool, rmb: bool, click: bool, wheel: i32, look: (i32, i32), size: Option<(i32, i32)> }
const K_BACK: u8 = 8; const K_TAB: u8 = 9; const K_ENTER: u8 = 13; const K_SHIFT: u8 = 16; const K_ESC: u8 = 27; const K_SPACE: u8 = 32;
const K_LEFT: u8 = 37; const K_UP: u8 = 38; const K_RIGHT: u8 = 39; const K_DOWN: u8 = 40; const K_F1: u8 = 0x70;
impl Input {
    pub fn new() -> Self { Input { down: vec![false; 256], hits: vec![], chars: vec![], mx: 0, my: 0, lmb: false, rmb: false, click: false, wheel: 0, look: (0, 0), size: None } }
    pub fn apply(&mut self, evs: Vec<Ev>) {
        for e in evs {
            match e {
                Ev::Key(k, d) => { if d { self.hits.push(k) } self.down[k as usize] = d }
                Ev::Char(c) => if c >= ' ' && c != '\u{7f}' { self.chars.push(c) },
                Ev::Mouse(x, y) => (self.mx, self.my) = (x, y),
                Ev::Btn(0, d) => { self.click |= d; self.lmb = d }
                Ev::Btn(_, d) => self.rmb = d,
                Ev::Wheel(w) => self.wheel += w,
                Ev::Size(w, h) => self.size = Some((w, h)),
                Ev::Blur => { self.down.fill(false); (self.lmb, self.rmb) = (false, false) }
            }
        }
    }
    fn hit(&self, k: u8) -> bool { self.hits.contains(&k) }
    fn held(&self, k: u8) -> bool { self.down[k as usize] }
    pub fn end_frame(&mut self) { self.hits.clear(); self.chars.clear(); self.click = false; self.wheel = 0; self.size = None }
}

// ─── FONT: битовый шрифт 5×10 (ячейка 6×11): ASCII 32..126, А..я, Ё, ё. Бит = строка*5 + столбец ──
const FONT: [u64; 161] = [
    0x0, 0x2008421080, 0x52940, 0x52beafa940, 0x23e8e2f880, 0xc644444c60, 0xb26a22a4c0, 0x11080, 0x4104211100,
    0x1110841040, 0x12aea9000, 0x109f21000, 0x22180000000, 0x1f00000, 0x3180000000, 0x44444000, 0x74675cc5c0, 0x7108421880,
    0xf8888845c0, 0x74608223e0, 0x423e953100, 0x74610787e0, 0x7462f08980, 0x10844443e0, 0x7462e8c5c0, 0x3221e8c5c0, 0x18c031800,
    0x110c031800, 0x4104111100, 0x3e0f8000, 0x1111041040, 0x20088845c0, 0x756b6845c0, 0x8c63f8c5c0, 0x7c62f8c5e0, 0x744210c5c0,
    0x3a6318a4e0, 0xf842f087e0, 0x842f087e0, 0xf463d0c5c0, 0x8c63f8c620, 0x71084211c0, 0x3250842380, 0x8a4a32a620, 0xf842108420,
    0x8c635aee20, 0x8c7359c620, 0x746318c5c0, 0x842f8c5e0, 0xb26b18c5c0, 0x8a4af8c5e0, 0x7c20e087c0, 0x21084213e0, 0x746318c620,
    0x22a318c620, 0x556b58c620, 0x8c54454620, 0x2108454620, 0xf8444443e0, 0x70842109c0, 0x410410400, 0x72108421c0, 0x8a880,
    0x1f0000000000, 0x41040, 0xf47d070000, 0x7c63178420, 0x7442170000, 0xf4631f4200, 0x707f170000, 0x1084714980, 0x1d187a31f0000,
    0x8c63368420, 0x7108430080, 0xc94210860100, 0x4946548420, 0x71084210c0, 0xad6b558000, 0x8c63368000, 0x7463170000, 0x217c63178000,
    0x210f4631f0000, 0x843368000, 0x7c1c1f0000, 0x6484238840, 0xb663188000, 0x22a3188000, 0x556b188000, 0x8a88a88000, 0x1d187a3188000,
    0xf8888f8000, 0x4108221100, 0x2108421080, 0x1108821040, 0x11510000, 0x8c63f8c5c0, 0x7c62f087e0, 0x7c62f8c5e0, 0x8421087e0,
    0x8fd4a529c0, 0xf842f087e0, 0xad5c4756a0, 0x7460c845c0, 0x8c675cc620, 0x8c675cc62e, 0x8a4a32a620, 0x8ca5294b80, 0x8c635aee20,
    0x8c63f8c620, 0x746318c5c0, 0x8c6318c7e0, 0x842f8c5e0, 0x744210c5c0, 0x21084213e0, 0x7461e8c620, 0x23ab5ab880, 0x8c54454620,
    0x87d294a520, 0x8421e8c620, 0xfd6b5ad6a0, 0x87eb5ad6a0, 0x74a4e10860, 0x9d6b38c620, 0x7c62f08420, 0x7461e845c0, 0x4d6b7ad520,
    0x8ca9e8c7c0, 0xf47d070000, 0x7462f0ba00, 0x7c5f178000, 0x8421f8000, 0x8fd4a70000, 0x707f170000, 0xad5d5a8000, 0x7459170000,
    0x9d73188000, 0x9d7318ba20, 0x4946548000, 0x8ca52e0000, 0x8c6bb88000, 0x8c7f188000, 0x7463170000, 0x8c631f8000, 0x217c63178000,
    0x7442170000, 0x21084f8000, 0x1d187a3188000, 0x84756b571000, 0x8a88a88000, 0x10fa52948000, 0x843d188000, 0xfd6b5a8000, 0x10fd6b5a8000,
    0x749c218000, 0x9d67188000, 0x7c5e108000, 0x7479170000, 0x4d6f548000, 0x8cbd1f0000, 0xf842f087ea, 0x707f172800,
];
fn glyph(c: char) -> u64 {
    let u = c as u32;
    FONT[match u { 32..=126 => u - 32, 0x410..=0x44F => u - 0x410 + 95, 0x401 => 159, 0x451 => 160, _ => 31 } as usize]
}

// ─── UI: холст, прямоугольники, текст, смешивание цветов ────────────────────────────────────────
pub struct Canvas { pub w: i32, pub h: i32, pub px: Vec<u32> }
impl Canvas {
    fn new(w: i32, h: i32) -> Self { Canvas { w, h, px: vec![0; (w * h) as usize] } }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        let (x0, y0, x1, y1) = (x.max(0), y.max(0), (x + w).min(self.w), (y + h).min(self.h));
        if x1 <= x0 { return }
        for yy in y0..y1 {
            for p in &mut self.px[(yy * self.w + x0) as usize..(yy * self.w + x1) as usize] { *p = if a >= 255 { c } else { blend(*p, c, a) } }
        }
    }
    fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        self.rect(x, y, w, 1, c, 255); self.rect(x, y + h - 1, w, 1, c, 255); self.rect(x, y, 1, h, c, 255); self.rect(x + w - 1, y, 1, h, c, 255);
    }
    fn ch(&mut self, x: i32, y: i32, c: char, col: u32, s: i32) {
        let g = glyph(c);
        for b in 0..50 { if g >> b & 1 == 1 { self.rect(x + (b % 5) * s, y + (b / 5) * s, s, s, col, 255) } }
    }
    fn text(&mut self, x: i32, y: i32, t: &str, col: u32, s: i32) -> i32 {
        let mut cx = x;
        for c in t.chars() { self.ch(cx, y, c, col, s); cx += 6 * s }
        cx
    }
}
fn tw(t: &str, s: i32) -> i32 { t.chars().count() as i32 * 6 * s }
fn blend(d: u32, s: u32, a: u32) -> u32 { let f = |sh: u32| ((((d >> sh) & 255) * (256 - a) + ((s >> sh) & 255) * a) >> 8) << sh; f(16) | f(8) | f(0) }
fn mul(c: u32, k: f32) -> u32 { let f = |sh: u32| (((((c >> sh) & 255) as f32) * k).clamp(0., 255.) as u32) << sh; f(16) | f(8) | f(0) }
fn mix(a: u32, b: u32, t: f32) -> u32 { blend(a, b, (t.clamp(0., 1.) * 256.) as u32) }
fn inside(x: i32, y: i32, b: (i32, i32, i32, i32)) -> bool { x >= b.0 && y >= b.1 && x < b.0 + b.2 && y < b.1 + b.3 }
const TOP: i32 = 54; const TXT: u32 = 0xe8eef2; const ACC: u32 = 0xffd24a; const RED: u32 = 0xff4a3a; const DIM: u32 = 0x8a96a0; const PANEL: u32 = 0x0e1216;

// ─── CONFIG: 6 ползунков (1..100%) → параметры движка; планеты ─────────────────────────────────
const S: i32 = 2; const SF: f32 = 2.; // плотность мира: 2 вокселя на метр по каждой оси = 8 вокселей на м³ (воксель 0.5 м)
const SL_NAME: [[&str; 2]; 6] = [["ОЗУ", "RAM"], ["ЦП", "CPU"], ["ГП", "GPU"], ["Накопитель", "Storage"], ["Кадры/с", "FPS"], ["Мир", "World"]];
const LEVELS: [[&str; 2]; 5] = [["Клеточная жизнь", "Cellular life"], ["Физика", "Physics"], ["Химия и огонь", "Chemistry & fire"], ["Организмы", "Organisms"], ["Общество людей", "Human society"]];
const MODES: [[&str; 2]; 3] = [["Текст", "Text"], ["2D", "2D"], ["3D", "3D"]];
const TOOLS: [[&str; 2]; 9] = [["Осмотр", "Inspect"], ["Взрыв", "Blast"], ["Материал", "Material"], ["Ластик", "Eraser"], ["Огонь", "Fire"], ["Клетки", "Cells"], ["Травоядное", "Herbivore"], ["Хищник", "Predator"], ["Человек", "Human"]];
#[derive(Clone, Copy)]
struct Q { radius: i32, mem_mb: f32, hist: usize, max_ent: usize, tick_hz: f32, level: u8, bfs: usize, nsup: usize, res: f32, view: f32, shadows: bool, tex: bool, water: bool, parts: usize, txt: i32, vpc: i32, depth2d: i32, mono: bool, chunks: usize, oct: u32, variety: f32, fps: f32, mode: u8 }
fn quality(s: &[i32; 6]) -> Q {
    let f = |i: usize| s[i] as f32 / 100.;
    let g = f(2);
    Q {
        radius: (16 + (s[0] - 1) * 96 / 99) * S, mem_mb: 0.5 * 2f32.powf(f(0) * 8.), hist: 10 + s[0] as usize * 2, max_ent: 20 + s[0] as usize * 3, // ОЗУ
        tick_hz: 4. + f(1) * 56., level: match s[1] { 0..=12 => 0, 13..=30 => 1, 31..=50 => 2, 51..=75 => 3, _ => 4 },                   // ЦП
        bfs: (400 + s[1] as usize * 200) * 4, nsup: 4 + s[1] as usize / 2,
        res: 0.15 + 0.85 * g, view: (16. + 94. * g) * SF, shadows: g > 0.5, tex: g > 0.25, water: g > 0.65, parts: (g * 3000.) as usize, // ГП
        txt: if g < 0.34 { 3 } else if g < 0.67 { 2 } else { 1 }, vpc: if g < 0.5 { 2 } else { 1 }, depth2d: (g * 24.) as i32, mono: g < 0.2,
        chunks: 1 + s[3] as usize * 15 / 100, oct: 1 + s[3] as u32 * 5 / 100, variety: f(3),                                                // Накопитель
        fps: (s[4] as f32 * 2.4).max(10.),                                                                                                 // Кадры/с
        mode: if s[5] < 34 { 0 } else if s[5] < 67 { 1 } else { 2 },                                                                      // Мир
    }
}
/// Планета: гравитация (м/с²), плотность атмосферы, цвета неба у горизонта/в зените, цвет поверхности с орбиты, «плавающий» мир.
#[derive(Clone, Copy)]
struct Planet { ru: &'static str, en: &'static str, g: f32, atm: f32, sky0: u32, sky1: u32, float: bool }
const PLANETS: [Planet; 4] = [
    Planet { ru: "Земля", en: "Earth", g: 9.8, atm: 1., sky0: 0xb8d4f0, sky1: 0x2f63bf, float: false },
    Planet { ru: "Луна", en: "Moon", g: 1.62, atm: 0., sky0: 0, sky1: 0, float: false },
    Planet { ru: "Марс", en: "Mars", g: 3.71, atm: 0.15, sky0: 0xd8a47a, sky1: 0x7a4a34, float: false },
    Planet { ru: "Пояс астероидов", en: "Asteroid belt", g: 0.4, atm: 0., sky0: 0, sky1: 0, float: true },
];

// ─── MATERIALS: вещества мира и их физические свойства ──────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum K { Air, Solid, Powder, Liquid, Gas, Fire, Life }
const KIND_NAME: [[&str; 2]; 7] = [["пустота", "empty"], ["твёрдое", "solid"], ["сыпучее", "powder"], ["жидкость", "liquid"], ["газ", "gas"], ["пламя", "flame"], ["клетка жизни", "life cell"]];
struct Mat { ru: &'static str, en: &'static str, sym: char, col: u32, k: K, dens: u8, stems: &'static str }
const fn mt(ru: &'static str, en: &'static str, sym: char, col: u32, k: K, dens: u8, stems: &'static str) -> Mat { Mat { ru, en, sym, col, k, dens, stems } }
const MATS: [Mat; 24] = [
    mt("Воздух", "Air", ' ', 0, K::Air, 0, "возд|пуст|ничт|вакуум|air|empty|nothing|vacuum"),
    mt("Коренная порода", "Bedrock", 'X', 0x2a2a2e, K::Solid, 255, "корен|bedrock"),
    mt("Камень", "Stone", '#', 0x7d7d82, K::Solid, 255, "камен|камн|stone|rock"),
    mt("Земля", "Dirt", '%', 0x7a5534, K::Solid, 255, "земл|грунт|почв|dirt|soil|earth"),
    mt("Трава", "Grass", '"', 0x4f9a3a, K::Solid, 255, "трав|grass"),
    mt("Песок", "Sand", ':', 0xd9c78a, K::Powder, 20, "песо|песк|sand"),
    mt("Вода", "Water", '~', 0x2f6fd0, K::Liquid, 10, "вод|water"),
    mt("Лава", "Lava", '~', 0xff5a12, K::Liquid, 14, "лав|магм|lava|magma"),
    mt("Дерево", "Wood", 'I', 0x7b5230, K::Solid, 255, "дерев|древ|бревн|доск|wood|log|plank|tree"),
    mt("Листва", "Leaves", '*', 0x2f7d2a, K::Solid, 255, "лист|leaf|leav"),
    mt("Огонь", "Fire", '^', 0xffb020, K::Fire, 0, "огон|огн|плам|fire|flame"),
    mt("Дым", "Smoke", '.', 0x55555a, K::Gas, 1, "дым|smoke"),
    mt("Пар", "Steam", '.', 0xc8d2dc, K::Gas, 1, "пар|steam"),
    mt("Лёд", "Ice", '_', 0xa8d8f0, K::Solid, 255, "лед|льд|ice"),
    mt("Снег", "Snow", '\'', 0xf2f6fa, K::Powder, 20, "снег|снеж|snow"),
    mt("Стекло", "Glass", '/', 0xbfe6ee, K::Solid, 255, "стекл|glass"),
    mt("Пепел", "Ash", ',', 0x9a9690, K::Powder, 20, "пеп|ash"),
    mt("Кирпич", "Brick", '=', 0xa8442e, K::Solid, 255, "кирпич|стен|brick|wall"),
    mt("Металл", "Metal", '+', 0xa0a8b4, K::Solid, 255, "металл|желез|сталь|metal|iron|steel"),
    mt("Щебень", "Gravel", ';', 0x8a8378, K::Powder, 20, "щеб|грав|облом|gravel|rubble|debris"),
    mt("Жизнь", "Life", 'O', 0x7dff4a, K::Life, 0, "жизн|клет|life|cell"),
    mt("Реголит", "Regolith", '.', 0x9c9a96, K::Solid, 255, "реголит|regolith"),
    mt("Марсианский песок", "Martian sand", ':', 0xb8623a, K::Powder, 20, "марсиан|ржав|martian|rust"),
    mt("Базальт", "Basalt", '#', 0x46444a, K::Solid, 255, "базальт|basalt"),
];
const AIR: u8 = 0; const BEDROCK: u8 = 1; const STONE: u8 = 2; const DIRT: u8 = 3; const GRASS: u8 = 4; const SAND: u8 = 5; const WATER: u8 = 6;
const LAVA: u8 = 7; const WOOD: u8 = 8; const LEAF: u8 = 9; const FIRE: u8 = 10; const SMOKE: u8 = 11; const STEAM: u8 = 12; const ICE: u8 = 13;
const SNOW: u8 = 14; const GLASS: u8 = 15; const BRICK: u8 = 17; const METAL: u8 = 18; const GRAVEL: u8 = 19; const LIFE: u8 = 20;
const REGOLITH: u8 = 21; const RSAND: u8 = 22; const BASALT: u8 = 23;
const MIXED: u8 = 254; // макро-блок 4³ из разных веществ
const VOID: u8 = 255;  // незагруженный чанк: твёрдый для физики, туман для рендера
fn kind(m: u8) -> K { MATS.get(m as usize).map_or(K::Solid, |x| x.k) }
fn dens(m: u8) -> u8 { MATS.get(m as usize).map_or(255, |x| x.dens) }
fn glow(m: u8) -> bool { matches!(m, FIRE | LAVA | LIFE) }
fn surf(m: u8) -> bool { !matches!(kind(m), K::Air | K::Gas | K::Fire) }

// ─── WORLD: кольцевой буфер 512×128×512 вокселей (256×64×256 м) вокруг игрока; чанки 32×128×32; ─
//            секции физики 16³; макро-сетка 4³ «однородный материал / смешанный» для пропуска пустоты
const W: usize = 512; const H: usize = 128; const WM: i32 = 511; const CB: i32 = 5; const CS: i32 = 32;
const NONE: (i32, i32) = (i32::MIN, i32::MIN);
const N6: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)]; // «вниз» последним: поиск опоры ныряет к земле
fn vi(x: i32, y: i32, z: i32) -> usize { ((z & WM) as usize * W + (x & WM) as usize) * H + y as usize }
fn ci(x: i32, z: i32) -> usize { (z & WM) as usize * W + (x & WM) as usize }
fn si(cx: i32, cz: i32) -> usize { ((cz & 15) * 16 + (cx & 15)) as usize }
fn sec(x: i32, y: i32, z: i32) -> usize { ((((z >> 4) & 31) * 32 + ((x >> 4) & 31)) * 8 + (y >> 4)) as usize }
fn mi(x: i32, y: i32, z: i32) -> usize { ((((z >> 2) & 127) * 128 + ((x >> 2) & 127)) * 32 + (y >> 2)) as usize }
pub struct World {
    v: Vec<u8>, st: Vec<u8>, top: Vec<u8>, mac: Vec<u8>, mq: Vec<bool>, recheck: Vec<u32>, sup: Vec<u8>, sq: Vec<bool>, srecheck: Vec<u32>, wakeq: VecDeque<usize>,
    slot: Vec<(i32, i32)>, modf: Vec<bool>, dirty: Vec<bool>, ndirty: Vec<bool>, mem: HashMap<(i32, i32), Vec<u8>>, memq: VecDeque<(i32, i32)>, mem_bytes: usize,
}
impl World {
    fn new() -> Self {
        let n = W * W * H;
        World { v: vec![0; n], st: vec![0; n], top: vec![0; W * W], mac: vec![MIXED; n / 64], mq: vec![false; n / 64], recheck: vec![], sup: vec![MIXED; 8192], sq: vec![false; 8192], srecheck: vec![], wakeq: VecDeque::new(),
            slot: vec![NONE; 256], modf: vec![false; 256], dirty: vec![false; 8192], ndirty: vec![false; 8192], mem: HashMap::new(), memq: VecDeque::new(), mem_bytes: 0 }
    }
    fn reset(&mut self) { self.slot.fill(NONE); self.modf.fill(false); self.mem.clear(); self.memq.clear(); self.mem_bytes = 0; self.wakeq.clear() }
    fn loaded(&self, x: i32, z: i32) -> bool { self.slot[si(x >> CB, z >> CB)] == (x >> CB, z >> CB) }
    fn get(&self, x: i32, y: i32, z: i32) -> u8 { if y < 0 { BEDROCK } else if y >= H as i32 { AIR } else if !self.loaded(x, z) { VOID } else { self.v[vi(x, y, z)] } }
    fn top(&self, x: i32, z: i32) -> i32 { self.top[ci(x, z)] as i32 }
    fn wake(&mut self, x: i32, y: i32, z: i32) { if (0..H as i32).contains(&y) && self.loaded(x, z) { self.ndirty[sec(x, y, z)] = true } }
    fn set(&mut self, x: i32, y: i32, z: i32, m: u8) -> bool {
        if y < 1 || y >= H as i32 || !self.loaded(x, z) { return false }
        let i = vi(x, y, z);
        if self.v[i] == m { return false }
        self.v[i] = m;
        let c = ci(x, z);
        if surf(m) { if y > self.top[c] as i32 { self.top[c] = y as u8 } } else if y == self.top[c] as i32 {
            let mut yy = y;
            while yy > 0 && !surf(self.v[vi(x, yy, z)]) { yy -= 1 }
            self.top[c] = yy as u8
        }
        self.wake(x, y, z);
        for (dx, dy, dz) in N6 { self.wake(x + dx, y + dy, z + dz) }
        let b = mi(x, y, z); // макро-блок больше не гарантированно однороден → в очередь перепроверки
        self.mac[b] = MIXED;
        if !self.mq[b] { self.mq[b] = true; self.recheck.push(b as u32) }
        self.sup[sec(x, y, z)] = MIXED; // и секция 16^3 (второй уровень сетки) — тоже
        self.modf[si(x >> CB, z >> CB)] = true;
        true
    }
    /// Материал макро-блока 4³ (по индексу кольцевого буфера), если он однороден, иначе MIXED.
    fn block_mat(&self, b: usize) -> u8 {
        let (by, bx, bz) = ((b % 32) as i32 * 4, ((b / 32) % 128) as i32 * 4, (b / 4096) as i32 * 4);
        let m0 = self.v[vi(bx, by, bz)];
        for dz in 0..4 { for dx in 0..4 { let base = vi(bx + dx, by, bz + dz); if self.v[base..base + 4].iter().any(|&m| m != m0) { return MIXED } } }
        m0
    }
    /// Секция 16^3 пуста, если все её 64 макро-блока 4^3 — воздух.
    fn sec_air(&self, s: usize) -> u8 {
        let (sy, sx, sz) = ((s % 8) as i32 * 16, ((s / 8) % 32) as i32 * 16, (s / 256) as i32 * 16);
        for dz in (0..16).step_by(4) { for dx in (0..16).step_by(4) { for dy in (0..16).step_by(4) { if self.mac[mi(sx + dx, sy + dy, sz + dz)] != AIR { return MIXED } } } }
        AIR
    }
    fn recheck(&mut self, budget: usize) -> usize {
        let n = budget.min(self.recheck.len());
        for _ in 0..n {
            let b = self.recheck.pop().unwrap_or(0) as usize;
            self.mq[b] = false;
            self.mac[b] = self.block_mat(b);
            if self.mac[b] == AIR { let s = ((b / 4096) / 4 * 32 + ((b / 32) % 128) / 4) * 8 + (b % 32) / 4; if !self.sq[s] { self.sq[s] = true; self.srecheck.push(s as u32) } }
        }
        for _ in 0..self.srecheck.len().min(512) { let s = self.srecheck.pop().unwrap_or(0) as usize; self.sq[s] = false; self.sup[s] = self.sec_air(s) }
        n
    }
    /// Выгружаемый изменённый чанк сжимается (RLE) в «память мира»; старые воспоминания вытесняются по лимиту ОЗУ.
    fn store(&mut self, s: usize, cap: usize) {
        let (cx, cz) = self.slot[s];
        let mut out: Vec<u8> = Vec::new();
        for z in cz * CS..cz * CS + CS { for x in cx * CS..cx * CS + CS {
            let b = vi(x, 0, z);
            for &m in &self.v[b..b + H] { let n = out.len(); if n >= 2 && out[n - 1] == m && out[n - 2] < 255 { out[n - 2] += 1 } else { out.push(1); out.push(m) } }
        } }
        self.mem_bytes += out.len();
        if let Some(o) = self.mem.insert((cx, cz), out) { self.mem_bytes -= o.len() }
        self.memq.push_back((cx, cz));
        while self.mem_bytes > cap { match self.memq.pop_front() { Some(k) => if let Some(o) = self.mem.remove(&k) { self.mem_bytes -= o.len() }, None => break } }
    }
    fn restore(&mut self, cx: i32, cz: i32) -> bool {
        let Some(d) = self.mem.remove(&(cx, cz)) else { return false };
        self.mem_bytes -= d.len();
        let mut it = d.chunks(2).flat_map(|p| std::iter::repeat_n(p[1], p[0] as usize));
        for z in cz * CS..cz * CS + CS { for x in cx * CS..cx * CS + CS { let b = vi(x, 0, z); for y in 0..H { self.v[b + y] = it.next().unwrap_or(AIR) } } }
        true
    }
}

// ─── GEN: процедурная генерация чанков (Земля, Луна, Марс, пояс астероидов) ─────────────────────
#[derive(Clone)]
struct Gen { seed: u64, pl: u8, sea: f32, base: f32, amp: f32, forest: f32, snow: bool, desert: bool, volcano: bool, life: f32, city: f32, caves: bool, ruins: bool, fauna: f32, people: f32 }
fn hash(a: i32, b: i32, c: i32, s: u64) -> u64 {
    let mut h = s ^ (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (c as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h = (h ^ (h >> 31)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 29)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 32)
}
fn hf(a: i32, b: i32, c: i32, s: u64) -> f32 { (hash(a, b, c, s) >> 40) as f32 / 16_777_216. }
fn noise(x: f32, y: f32, z: f32, s: u64) -> f32 {
    let (xi, yi, zi) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let sm = |t: f32| t * t * (3. - 2. * t);
    let (u, v, w) = (sm(x - xi as f32), sm(y - yi as f32), sm(z - zi as f32));
    let c = |a: i32, b: i32, d: i32| hf(xi + a, yi + b, zi + d, s);
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    l(l(l(c(0, 0, 0), c(1, 0, 0), u), l(c(0, 1, 0), c(1, 1, 0), u), v), l(l(c(0, 0, 1), c(1, 0, 1), u), l(c(0, 1, 1), c(1, 1, 1), u), v), w)
}
fn fbm(x: f32, z: f32, oct: u32, s: u64) -> f32 {
    let (mut a, mut f, mut sum, mut n) = (1., 1., 0., 0.);
    for o in 0..oct { sum += a * noise(x * f, z * f, 0., s + o as u64); n += a; a *= 0.5; f *= 2. }
    sum / n
}
impl Gen {
    fn base(seed: u64) -> Gen { Gen { seed, pl: 0, sea: 22., base: 25., amp: 12., forest: 0.45, snow: false, desert: false, volcano: false, life: 0.35, city: 0., caves: false, ruins: false, fauna: 1., people: 0.4 } }
    fn sea_v(&self) -> i32 { if self.pl == 0 { (self.sea * SF) as i32 } else { -1 } }
    /// Высота рельефа в вокселях и расстояние (м) до жерла вулкана. Параметры Gen заданы в метрах.
    fn height(&self, x: i32, z: i32, oct: u32) -> (i32, f32) {
        let (mx, mz) = (x as f32 / SF, z as f32 / SF);
        let mut h = self.base + (fbm(mx / 72., mz / 72., oct, self.seed) - 0.5) * 2.4 * self.amp;
        let mut vd = 1e9;
        if self.pl == 1 { // лунные кратеры: чаша и вал
            let (gx, gz) = ((mx / 40.).floor() as i32, (mz / 40.).floor() as i32);
            for dz in -1..=1 { for dx in -1..=1 {
                let (cx, cz) = (gx + dx, gz + dz);
                let (px, pz, r) = (cx as f32 * 40. + hf(cx, cz, 21, self.seed) * 40., cz as f32 * 40. + hf(cx, cz, 22, self.seed) * 40., 4. + hf(cx, cz, 23, self.seed) * 12.);
                let d = ((mx - px).powi(2) + (mz - pz).powi(2)).sqrt() / r;
                if d < 1. { h -= (1. - d * d) * r * 0.45 } else if d < 1.4 { h += (1. - (d - 1.) / 0.4) * r * 0.12 }
            } }
        }
        if self.pl == 2 { h -= (0.06 - (noise(mx / 160., mz / 160., 0., self.seed + 31) - 0.5).abs()).max(0.) * 260. } // марсианские каньоны
        if self.volcano && self.pl == 0 {
            let (gx, gz) = ((mx / 160.).floor() as i32, (mz / 160.).floor() as i32);
            let (vx, vz) = (gx as f32 * 160. + 40. + hf(gx, gz, 1, self.seed) * 80., gz as f32 * 160. + 40. + hf(gx, gz, 2, self.seed) * 80.);
            vd = ((mx - vx).powi(2) + (mz - vz).powi(2)).sqrt();
            if vd < 34. { h = h.max(self.base) + 34. - vd; if vd < 5. { h -= (5. - vd) * 2.5 } }
        }
        (((h * SF) as i32).clamp(3, H as i32 - 12), vd)
    }
}
/// Генерирует (или восстанавливает из памяти) чанк; возвращает существ для заселения: (вид, позиция, стадо).
fn gen_chunk(w: &mut World, g: &Gen, q: &Q, cx: i32, cz: i32) -> Vec<(u8, V3, u32)> {
    let s = si(cx, cz);
    if w.slot[s] != NONE && w.modf[s] { w.store(s, (q.mem_mb * 1_048_576.) as usize) }
    w.slot[s] = (cx, cz);
    let restored = w.restore(cx, cz);
    let col = |lx: i32, lz: i32| (cx * CS + lx, cz * CS + lz);
    let idx = |lx: i32, lz: i32| (lz * CS + lx) as usize;
    let mut spawn = vec![];
    if !restored && g.pl == 3 { // пояс астероидов: каменные глыбы в пустоте, шум считается блоками 2³
        for lz in (0..CS).step_by(2) { for lx in (0..CS).step_by(2) { for y in (0..H as i32).step_by(2) {
            let (x, z) = col(lx, lz);
            let band = 1. - ((y as f32 - H as f32 / 2.) / (H as f32 / 2. - 6.)).abs().min(1.).powi(4);
            let n = (noise(x as f32 / 40., y as f32 / 40., z as f32 / 40., g.seed) * 0.65 + noise(x as f32 / 12., y as f32 / 12., z as f32 / 12., g.seed + 1) * 0.35) * band;
            let r = hf(x, y, z, g.seed + 2);
            let m = if n < 0.6 { AIR } else if n < 0.64 { REGOLITH } else if r < 0.03 { METAL } else if r < 0.08 { ICE } else if n > 0.7 { BASALT } else { STONE };
            for (dx, dy, dz) in [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)] { w.v[vi(x + dx, y + dy, z + dz)] = m }
        } } }
    } else if !restored {
        let sea = g.sea_v();
        let mut hs = vec![0i32; (CS * CS) as usize];
        for lz in 0..CS { for lx in 0..CS {
            let (x, z) = col(lx, lz);
            let (h, vd) = g.height(x, z, q.oct);
            hs[idx(lx, lz)] = h;
            let (top, sub, deep) = match g.pl {
                1 => (REGOLITH, REGOLITH, if hf(x >> 4, 0, z >> 4, g.seed + 7) < 0.4 { BASALT } else { STONE }),
                2 => (if h > (52. * SF) as i32 { ICE } else { RSAND }, BASALT, BASALT),
                _ => {
                    let t = if vd < 5. { LAVA } else if g.desert || h <= sea + 2 { if h < sea - 6 && hf(x, z, 1, g.seed) < 0.3 { GRAVEL } else { SAND } }
                        else if g.snow || h > (46. * SF) as i32 { SNOW } else { GRASS };
                    (t, if vd < 5. { LAVA } else if t == SAND || t == GRAVEL || g.desert { SAND } else { DIRT }, STONE)
                }
            };
            let b = vi(x, 0, z);
            for y in 0..H as i32 {
                w.v[b + y as usize] = if y == 0 { BEDROCK } else if y < h - 6 {
                    if g.caves && y > 4 && noise(x as f32 / 28., y as f32 / 18., z as f32 / 28., g.seed + 9) > 0.72 { AIR }
                    else if q.variety > 0.6 && hf(x, y, z, g.seed + 5) < 0.003 { METAL } else { deep }
                } else if y < h { sub } else if y == h { top } else if y <= sea { if g.snow && y == sea { ICE } else { WATER } } else { AIR };
            }
        } }
        if g.pl == 0 {
            for lz in 5..CS - 5 { for lx in 5..CS - 5 { // деревья: при высоком «Накопителе» выше, разнообразнее, бывают толстые
                let (x, z) = col(lx, lz);
                let h = hs[idx(lx, lz)];
                if !matches!(w.v[vi(x, h, z)], GRASS | SNOW) || h <= sea + 2 || h > H as i32 - 24 || hf(x, z, 7, g.seed) >= g.forest * 0.008 { continue }
                let big = q.variety > 0.5 && hf(x, z, 6, g.seed) < 0.3;
                let th = 8 + if q.variety > 0.4 { (hash(x, z, 8, g.seed) % 5) as i32 } else { 0 } + if big { 3 } else { 0 };
                let rr = if big { 5 } else { 4 };
                for y in h + 1..=h + th { for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().take(if big { 4 } else { 1 }) { w.v[vi(x + dx, y, z + dz)] = WOOD } }
                for dy in -rr..=rr { for dz in -rr..=rr { for dx in -rr..=rr {
                    let (xx, y, zz) = (x + dx, h + th + dy, z + dz);
                    if dx * dx + dy * dy + dz * dz > rr * rr + 1 || y >= H as i32 - 1 || (q.variety > 0.7 && hf(xx, y, zz, g.seed) < 0.12) { continue }
                    let i = vi(xx, y, zz);
                    if w.v[i] == AIR { w.v[i] = LEAF }
                } } }
            } }
            let ruin = g.ruins && hf(cx, cz, 5, g.seed) < 0.6;
            if (g.city > 0. && hf(cx, cz, 3, g.seed) < g.city * 0.45) || (g.ruins && hf(cx, cz, 4, g.seed) < 0.3) { // дом: кирпич, окна, дверь, двускатная крыша
                let h0 = hs[idx(16, 16)];
                let (sx, sz) = (12 + (hash(cx, cz, 6, g.seed) % 7) as i32, 12 + (hash(cx, cz, 7, g.seed) % 7) as i32);
                let (x0, z0, x1, z1) = (6, 6, 6 + sx - 1, 6 + sz - 1);
                if h0 > sea + 1 && h0 < H as i32 - 32 {
                    for lz in z0 - 1..=z1 + 1 { for lx in x0 - 1..=x1 + 1 {
                        let (x, z) = col(lx, lz);
                        let inside = lx >= x0 && lx <= x1 && lz >= z0 && lz <= z1;
                        let edge = inside && (lx == x0 || lx == x1 || lz == z0 || lz == z1);
                        let corner = (lx == x0 || lx == x1) && (lz == z0 || lz == z1);
                        let gy = hs[idx(lx, lz)];
                        for y in 1..H as i32 {
                            let k = y - (h0 + 9);
                            let mut m = if y <= h0 { if inside && y > gy { STONE } else { continue } }
                                else if y <= h0 + 8 {
                                    if !inside { if y > gy { AIR } else { continue } } else if !edge { AIR }
                                    else if lz == z0 && (lx - x0 - sx / 2).abs() <= 1 && y <= h0 + 5 { AIR }
                                    else if (y == h0 + 4 || y == h0 + 5) && !corner && ((lx - x0) % 4 == 2 || (lz - z0) % 4 == 2) { GLASS } else { BRICK }
                                } else if k <= sz / 2 + 3 {
                                    if ruin { AIR } else if k <= sz / 2 + 1 && (lz == z0 - 1 + k || lz == z1 + 1 - k) { BRICK }
                                    else if k <= sz / 2 && (lx == x0 || lx == x1) && lz > z0 - 1 + k && lz < z1 + 1 - k { WOOD } else { AIR }
                                } else { continue };
                            if ruin && m != AIR && y > h0 && hf(x, y, z, g.seed + 3) < 0.45 { m = if y == h0 + 1 { GRAVEL } else { AIR } }
                            w.v[vi(x, y, z)] = m;
                        }
                    } }
                    let (dx, dz) = col(x0 + sx / 2, z0 - 4);
                    for i in 0..2 + (hash(cx, cz, 9, g.seed) % 2) as i32 { spawn.push((2, [dx as f32 + i as f32 * 2., 0., dz as f32 + 0.5], 0)) }
                }
            }
            if g.life > 0. { for lz in 0..CS { for lx in 0..CS { // очаги клеточной жизни (Конвей)
                let (x, z) = col(lx, lz);
                let h = (0..H as i32).rev().find(|&y| surf(w.v[vi(x, y, z)])).unwrap_or(0);
                if matches!(w.v[vi(x, h, z)], GRASS | DIRT | SAND | SNOW) && h + 1 < H as i32 && w.v[vi(x, h + 1, z)] == AIR
                    && noise(x as f32 / 20., z as f32 / 20., 0., g.seed + 3) > 1. - g.life * 0.5 && hf(x, z, 9, g.seed) < 0.4 { w.v[vi(x, h + 1, z)] = LIFE }
            } } }
            let (mx, mz) = col(16, 16); // фауна: стада травоядных, хищники, одиночки-исследователи
            let grp = (hash(cx, cz, 14, g.seed) & 0xffff) as u32 + 1;
            if w.v[vi(mx, hs[idx(16, 16)], mz)] == GRASS && hf(cx, cz, 11, g.seed) < 0.12 * g.fauna {
                for i in 0..3 + (hash(cx, cz, 15, g.seed) % 4) as i32 { spawn.push((0, [(mx + (i % 3) * 3 - 3) as f32, 0., (mz + (i / 3) * 3) as f32], grp)) }
            }
            if hf(cx, cz, 12, g.seed) < 0.04 * g.fauna { for i in 0..1 + (hash(cx, cz, 16, g.seed) % 2) as i32 { spawn.push((1, [(mx + 4 * i) as f32, 0., mz as f32], 0)) } }
            if hf(cx, cz, 13, g.seed) < 0.05 * g.people { spawn.push((2, [mx as f32 + 5., 0., mz as f32 - 5.], 0)) }
        }
    }
    for lz in 0..CS { for lx in 0..CS { let (x, z) = col(lx, lz); w.top[ci(x, z)] = (0..H as i32).rev().find(|&y| surf(w.v[vi(x, y, z)])).unwrap_or(0) as u8 } }
    for bz in 0..CS / 4 { for bx in 0..CS / 4 { for by in 0..H as i32 / 4 { let b = mi(cx * CS + bx * 4, by * 4, cz * CS + bz * 4); w.mac[b] = w.block_mat(b) } } }
    for sz in 0..2 { for sx in 0..2 { for sy in 0..8 { let s = sec(cx * CS + sx * 16, sy * 16, cz * CS + sz * 16); w.sup[s] = w.sec_air(s) } } }
    for sz in 0..2 { for sx in 0..2 { for sy in 0..8 { // будим постепенно только секции с подвижным веществом
        let (x0, y0, z0) = (cx * CS + sx * 16, sy * 16, cz * CS + sz * 16);
        let mobile = restored || (0..16).any(|dz| (0..16).any(|dx| { let b = vi(x0 + dx, y0, z0 + dz); w.v[b..b + 16].iter().any(|&m| !matches!(kind(m), K::Air | K::Solid)) }));
        if mobile { w.wakeq.push_back(sec(x0, y0, z0)) }
    } } }
    w.modf[s] = restored;
    spawn
}

// ─── SIM: 0 — жизнь Конвея, 1 — сыпучие/жидкости/газы и обрушения, 2 — правила-реакции, ─────────
//          3 — существа (травоядные и хищники), 4 — люди; космофизика: гравитация и вакуум планеты
struct Rule { a: u8, b: u8, c: u8, p: f32 } // «A (рядом с B) → C с шансом p за тик»; b = VOID — без соседа
struct Body { vox: Vec<([i32; 3], u8)>, v: f32, acc: f32, fell: i32 }
#[derive(Default, Clone, Copy)]
struct RStats { steps: u64, skips: u64, big: u64 }
#[derive(Clone)]
struct SimStats { secs: usize, cells: u64, sup_n: usize, sup_ms: f32, phys_ms: f32, life_ms: f32, ai_ns: [f32; 3], ai_n: [usize; 3], visits: Vec<u32>, mat_ns: Vec<f32> }
const ST_NAME: [[&str; 2]; 14] = [
    ["бродит", "wandering"], ["пасётся", "grazing"], ["ищет воду", "looking for water"], ["пьёт", "drinking"], ["убегает от хищника", "fleeing a predator"],
    ["держится стада", "staying with the herd"], ["патрулирует территорию", "patrolling territory"], ["охотится", "hunting"], ["ест добычу", "eating prey"],
    ["исследует мир", "exploring"], ["рубит дерево", "chopping wood"], ["строит хижину", "building a hut"], ["избегает опасности", "avoiding danger"], ["отдыхает", "resting"],
];
const S_WANDER: u8 = 0; const S_GRAZE: u8 = 1; const S_SEEKW: u8 = 2; const S_DRINK: u8 = 3; const S_FLEE: u8 = 4; const S_HERD: u8 = 5; const S_PATROL: u8 = 6;
const S_HUNT: u8 = 7; const S_EAT: u8 = 8; const S_EXPLORE: u8 = 9; const S_CHOP: u8 = 10; const S_BUILD: u8 = 11; const S_AVOID: u8 = 12; const S_REST: u8 = 13;
const CRE_NAME: [[&str; 2]; 3] = [["Травоядное (олень)", "Herbivore (deer)"], ["Хищник (волк)", "Predator (wolf)"], ["Человек", "Human"]];
/// Существо: позиция ног в вокселях, потребности 0..1, состояние ИИ, цель, «дом» (центр территории), стоимость обновления в нс.
#[derive(Clone)]
struct Cre { id: u32, k: u8, p: V3, vy: f32, yaw: f32, spd: f32, st: u8, tgt: V3, food: f32, water: f32, hp: f32, stam: f32, age: f32, group: u32, home: V3, inv: u8, built: u32, think: f32, ns: f32, alive: bool, col: u32, other: u32 }
fn dist_h(a: V3, b: V3) -> f32 { (a[0] - b[0]).hypot(a[2] - b[2]) }
fn away(p: V3, from: V3, d: f32) -> V3 { let (dx, dz) = (p[0] - from[0], p[2] - from[2]); let l = dx.hypot(dz).max(0.01); [p[0] + dx / l * d, p[1], p[2] + dz / l * d] }
pub struct Sim {
    w: World, rules: Vec<Rule>, by_a: Vec<Vec<usize>>, by_b: Vec<Vec<usize>>, bodies: Vec<Body>, claimed: HashSet<[i32; 3]>, supq: VecDeque<[i32; 3]>,
    tick: u32, rng: u64, fx: Vec<([f32; 3], u32)>, ev: Vec<(String, String)>, cre: Vec<Cre>, next_id: u32, pl: Planet, vc: u32, stt: SimStats,
}
impl Sim {
    fn new() -> Self {
        let stt = SimStats { secs: 0, cells: 0, sup_n: 0, sup_ms: 0., phys_ms: 0., life_ms: 0., ai_ns: [0.; 3], ai_n: [0; 3], visits: vec![0; MATS.len()], mat_ns: vec![0.; MATS.len()] };
        let mut s = Sim { w: World::new(), rules: vec![], by_a: vec![], by_b: vec![], bodies: vec![], claimed: HashSet::new(), supq: VecDeque::new(), tick: 0,
            rng: 0x2545_F491_4F6C_DD1D, fx: vec![], ev: vec![], cre: vec![], next_id: 1, pl: PLANETS[0], vc: 0, stt };
        s.default_rules();
        s
    }
    fn default_rules(&mut self) { self.rules = DEFAULT_RULES.iter().filter_map(|r| parse_rule(r)).collect(); self.reindex() }
    fn reindex(&mut self) {
        (self.by_a, self.by_b) = (vec![vec![]; MATS.len()], vec![vec![]; MATS.len()]);
        for (i, r) in self.rules.iter().enumerate() { if r.b == VOID { self.by_a[r.a as usize].push(i) } else { self.by_b[r.b as usize].push(i) } }
    }
    fn rnd(&mut self) -> u32 { self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17; (self.rng >> 32) as u32 }
    fn chance(&mut self, p: f32) -> bool { (self.rnd() as f32) < p * 4_294_967_296. }
    fn rf(&mut self) -> f32 { self.rnd() as f32 / 4_294_967_296. }
    fn rand_near(&mut self, p: V3, r: f32) -> V3 { let (a, b) = (self.rf() * 2. - 1., self.rf() * 2. - 1.); [p[0] + a * r, p[1], p[2] + b * r] }
    /// Запись вещества с проверкой опоры: если исчез твёрдый блок — соседи ставятся в очередь проверки обрушения.
    fn put(&mut self, x: i32, y: i32, z: i32, m: u8) {
        let o = self.w.get(x, y, z);
        if o == BEDROCK || o == VOID || !self.w.set(x, y, z, m) { return }
        if kind(o) == K::Solid && kind(m) != K::Solid {
            for (dx, dy, dz) in N6 { if dy >= 0 && kind(self.w.get(x + dx, y + dy, z + dz)) == K::Solid { if self.supq.len() >= 4096 { self.supq.pop_front(); } self.supq.push_back([x + dx, y + dy, z + dz]) } }
        }
    }
    /// Уровень 0: игра «Жизнь» (B3/S23) на верхней поверхности мира в радиусе ОЗУ.
    fn life_tick(&mut self, c: (i32, i32), r: i32) {
        let t0 = Instant::now();
        let w = &self.w;
        let mut ch = vec![];
        for z in c.1 - r..=c.1 + r { for x in c.0 - r..=c.0 + r {
            if !w.loaded(x, z) { continue }
            let t = w.top(x, z);
            let alive = w.get(x, t, z) == LIFE;
            let mut n = 0;
            for dz in -1..=1 { for dx in -1..=1 { if (dx != 0 || dz != 0) && w.get(x + dx, w.top(x + dx, z + dz), z + dz) == LIFE { n += 1 } } }
            if alive && !(2..=3).contains(&n) { ch.push((x, t, z, AIR)) }
            else if !alive && n == 3 && t + 1 < H as i32 && matches!(kind(w.get(x, t, z)), K::Solid | K::Powder) { ch.push((x, t + 1, z, LIFE)) }
        } }
        for (x, y, z, m) in ch { self.w.set(x, y, z, m); }
        self.stt.life_ms = ms(t0);
    }
    /// Уровни 1+: обход «проснувшихся» секций 16³ в радиусе — падение, течение, всплытие, реакции, обрушения.
    fn phys_tick(&mut self, c: (i32, i32), q: &Q) {
        let t_start = Instant::now();
        self.tick = self.tick.wrapping_add(1);
        let t = (self.tick % 255 + 1) as u8;
        for _ in 0..64 { match self.w.wakeq.pop_front() { Some(s) => self.w.ndirty[s] = true, None => break } } // постепенное «пробуждение» новых чанков
        std::mem::swap(&mut self.w.dirty, &mut self.w.ndirty);
        self.w.ndirty.fill(false);
        let rs = ((q.radius >> 4) + 1).min(15);
        let (psx, psz) = (c.0 >> 4, c.1 >> 4);
        let mut secs = vec![];
        for sy in 0..8 { for sz in psz - rs..=psz + rs { for sx in psx - rs..=psx + rs {
            let i = (((sz & 31) * 32 + (sx & 31)) * 8 + sy) as usize;
            if self.w.dirty[i] && self.w.loaded(sx << 4, sz << 4) { self.w.dirty[i] = false; secs.push((sx, sz, sy)) }
        } } }
        for i in 0..8192 { if self.w.dirty[i] { self.w.ndirty[i] = true } } // вне радиуса — ждут игрока
        self.stt.visits.fill(0);
        let cells0 = self.vc;
        for &(sx, sz, sy) in &secs { for y in (sy * 16).max(1)..sy * 16 + 16 { for z in sz * 16..sz * 16 + 16 { for x in sx * 16..sx * 16 + 16 { self.cell(x, y, z, t, q.level, false) } } } }
        for &(sx, sz, sy) in secs.iter().rev() { for y in ((sy * 16).max(1)..sy * 16 + 16).rev() { for z in sz * 16..sz * 16 + 16 { for x in sx * 16..sx * 16 + 16 { self.cell(x, y, z, t, q.level, true) } } } }
        self.bodies_tick();
        let t_sup = Instant::now();
        let (mut seen, mut budget) = (HashSet::new(), q.nsup);
        while budget > 0 {
            let Some(p) = self.supq.pop_front() else { break };
            if seen.contains(&p) || kind(self.w.get(p[0], p[1], p[2])) != K::Solid { continue }
            self.support(p, q.bfs, &mut seen);
            budget -= 1;
        }
        (self.stt.sup_n, self.stt.sup_ms, self.stt.secs) = (q.nsup - budget, ms(t_sup), secs.len());
        self.stt.cells = self.vc.wrapping_sub(cells0) as u64;
        self.stt.phys_ms = ms(t_start);
    }
    fn cell(&mut self, x: i32, y: i32, z: i32, t: u8, lvl: u8, gas: bool) {
        let i = vi(x, y, z);
        let m = self.w.v[i];
        if m == AIR || self.w.st[i] == t { return }
        let k = kind(m);
        if gas {
            if k == K::Gas { if self.pl.atm < 0.3 && self.chance(0.25) { self.w.set(x, y, z, AIR); } else { self.flow(x, y, z, m, t, 2); } }
            return;
        }
        self.vc = self.vc.wrapping_add(1);
        self.stt.visits[m as usize] += 1;
        let t0 = (self.vc & 31 == 0).then(Instant::now); // выборочный замер стоимости обхода по веществам (для инспектора)
        self.cell_do(x, y, z, m, k, t, lvl);
        if let Some(t0) = t0 { let e = &mut self.stt.mat_ns[m as usize]; *e = *e * 0.95 + t0.elapsed().as_nanos() as f32 * 0.05 }
    }
    fn cell_do(&mut self, x: i32, y: i32, z: i32, m: u8, k: K, t: u8, lvl: u8) {
        let (g, atm) = (self.pl.g, self.pl.atm);
        if atm < 0.3 { // вакуум/разрежение: пламя гаснет, вода выкипает
            if k == K::Fire { self.w.wake(x, y, z); if self.chance(0.5 * (1. - atm / 0.3)) { self.put(x, y, z, AIR); return } }
            if m == WATER && atm < 0.05 { self.w.wake(x, y, z); if self.chance(0.03) { self.put(x, y, z, STEAM); return } }
        }
        if lvl >= 2 && self.react(x, y, z, m, t) { return }
        if matches!(k, K::Powder | K::Liquid) && g < 9. && !self.chance(g / 8.) { // низкая гравитация: падение реже; спит, если двигаться некуда
            let d = dens(m);
            if dens(self.w.get(x, y - 1, z)) < d || (k == K::Liquid && N6[..4].iter().any(|&(dx, _, dz)| dens(self.w.get(x + dx, y, z + dz)) < d)) { self.w.wake(x, y, z) }
            return;
        }
        match k {
            K::Powder => { self.flow(x, y, z, m, t, 0); }
            K::Liquid => if m != LAVA || self.chance(0.3) { self.flow(x, y, z, m, t, 1); } else { self.w.wake(x, y, z) },
            _ => {}
        }
    }
    /// mode: 0 — сыпучее (вниз и по диагонали), 1 — жидкость (+ растекание), 2 — газ (вверх и в стороны).
    fn flow(&mut self, x: i32, y: i32, z: i32, m: u8, t: u8, mode: u8) -> bool {
        let (d, r) = (dens(m), self.rnd() as usize);
        for s in 0..if mode == 0 { 5 } else { 9 } {
            let (a, b) = [(1, 0), (0, 1), (-1, 0), (0, -1)][(r + s) & 3];
            let up = if mode == 2 { 1 } else { -1 };
            let (dx, dy, dz) = if s == 0 { (0, up, 0) } else if s < 5 { (a, up, b) } else { (a, 0, b) };
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            let o = self.w.get(nx, ny, nz);
            if if mode == 2 { o == AIR } else { dens(o) < d } {
                if mode == 2 && !self.chance(0.7) { self.w.wake(x, y, z); return false }
                self.w.set(x, y, z, if matches!(kind(o), K::Fire | K::Life) { AIR } else { o });
                if self.w.set(nx, ny, nz, m) { self.w.st[vi(nx, ny, nz)] = t }
                return true;
            }
        }
        false
    }
    /// Правила-реакции (уровень 2+). Вещество-«триггер» B меняет соседей A → C; самопроизвольные A → C.
    fn react(&mut self, x: i32, y: i32, z: i32, m: u8, t: u8) -> bool {
        let mi = m as usize;
        let (na, nb) = (self.by_a[mi].len(), self.by_b[mi].len());
        if na + nb == 0 { return false }
        self.w.wake(x, y, z);
        for j in 0..na { let r = self.by_a[mi][j]; let (c, p) = (self.rules[r].c, self.rules[r].p); if self.chance(p) { self.put(x, y, z, c); return true } }
        for (dx, dy, dz) in N6 {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            let o = self.w.get(nx, ny, nz);
            for j in 0..nb {
                let r = self.by_b[mi][j];
                if self.rules[r].a != o { continue }
                let (c, p) = (self.rules[r].c, self.rules[r].p);
                if self.chance(p) { self.put(nx, ny, nz, c); if ny > 0 && ny < H as i32 { self.w.st[vi(nx, ny, nz)] = t } break }
            }
        }
        false
    }
    fn bodies_tick(&mut self) {
        let acc = 0.08 * self.pl.g / 9.8;
        let mut bs = std::mem::take(&mut self.bodies);
        bs.retain_mut(|b| {
            b.v = (b.v + acc).min(2.);
            b.acc += b.v;
            while b.acc >= 1. {
                b.acc -= 1.;
                for &(p, _) in &b.vox { self.w.set(p[0], p[1], p[2], AIR); }
                let free = b.vox.iter().all(|&(p, _)| p[1] > 1 && dens(self.w.get(p[0], p[1] - 1, p[2])) < 20);
                if free { for (p, _) in b.vox.iter_mut() { p[1] -= 1 } b.fell += 1 }
                for &(p, m) in &b.vox { self.w.set(p[0], p[1], p[2], m); }
                if !free {
                    if b.fell > 4 { // удар о землю: часть массы рассыпается в щебень
                        if b.vox.len() >= 30 { self.ev.push((format!("Обрушение: {} вокс. упали с высоты {:.1} м", b.vox.len(), b.fell as f32 / SF), format!("Collapse: {} voxels fell {:.1} m", b.vox.len(), b.fell as f32 / SF))) }
                        for &(p, m) in &b.vox { if self.chance(0.4) { self.w.set(p[0], p[1], p[2], if m == LEAF { AIR } else { GRAVEL }); if self.chance(0.1) { self.fx.push(([p[0] as f32 + 0.5, p[1] as f32 + 0.5, p[2] as f32 + 0.5], MATS[m as usize].col)) } } }
                    }
                    return false;
                }
            }
            true
        });
        self.bodies = bs;
        self.claimed = self.bodies.iter().flat_map(|b| b.vox.iter().map(|v| v.0)).collect(); // воксели в падении не ищутся повторно
    }
    /// Поиск связной твёрдой массы без опоры (заливка до lim вокселей) → падающее тело.
    fn support(&mut self, s: [i32; 3], lim: usize, seen: &mut HashSet<[i32; 3]>) {
        let m0 = self.w.get(s[0], s[1], s[2]);
        if kind(m0) != K::Solid || m0 == VOID || m0 == BEDROCK || self.claimed.contains(&s) { return }
        let (mut set, mut st, mut grounded) = (HashSet::from([s]), vec![s], false);
        'bfs: while let Some(p) = st.pop() {
            for (dx, dy, dz) in N6 {
                let n = [p[0] + dx, p[1] + dy, p[2] + dz];
                let m = self.w.get(n[0], n[1], n[2]);
                if m == BEDROCK || m == VOID || seen.contains(&n) || self.claimed.contains(&n) || set.len() > lim { grounded = true; break 'bfs }
                if kind(m) == K::Solid && set.insert(n) { st.push(n) }
            }
        }
                let vox: Vec<_> = set.iter().map(|&p| (p, self.w.get(p[0], p[1], p[2]))).collect();
        seen.extend(set);
        if grounded || self.bodies.len() > 64 { return }
        if !vox.iter().all(|&(p, _)| { let b = [p[0], p[1] - 1, p[2]]; p[1] > 1 && (seen.contains(&b) || dens(self.w.get(b[0], b[1], b[2])) < 20) }) { return } // лежит на песке/воде — держится
        self.claimed.extend(vox.iter().map(|v| v.0));
        self.bodies.push(Body { vox, v: 0., acc: 0., fell: 0 });
    }
    fn explode(&mut self, c: [i32; 3], r: i32, lvl: u8) {
        let mut shell = vec![];
        for dz in -r - 1..=r + 1 { for dy in -r - 1..=r + 1 { for dx in -r - 1..=r + 1 {
            let (d2, p) = (dx * dx + dy * dy + dz * dz, [c[0] + dx, c[1] + dy, c[2] + dz]);
            let m = self.w.get(p[0], p[1], p[2]);
            if d2 <= r * r {
                if m == AIR || m == BEDROCK || m == VOID { continue }
                if self.chance(0.08) { self.fx.push(([p[0] as f32 + 0.5, p[1] as f32 + 0.5, p[2] as f32 + 0.5], MATS[m as usize].col)) }
                let fire = lvl >= 2 && d2 >= (r - 1) * (r - 1) && self.chance(0.15);
                self.w.set(p[0], p[1], p[2], if fire { FIRE } else { AIR });
            } else if d2 <= (r + 1) * (r + 1) && kind(m) == K::Solid && m != VOID { shell.push(p) }
        } } }
        if lvl >= 1 { self.supq.extend(shell); while self.supq.len() > 4096 { self.supq.pop_front(); } }
        let cf = [c[0] as f32, c[1] as f32, c[2] as f32];
        let mut dead = 0;
        for k in 0..self.cre.len() {
            let p = self.cre[k].p;
            if self.cre[k].alive && dot(sub(p, cf), sub(p, cf)) < ((r + 3) * (r + 3)) as f32 {
                self.cre[k].alive = false;
                dead += 1;
                for _ in 0..6 { self.fx.push(([p[0], p[1] + 1., p[2]], 0xa02020)) }
            }
        }
        if dead > 0 { self.ev.push((format!("Взрывом убито существ: {}", dead), format!("Creatures killed by the blast: {}", dead))) }
    }

    // ── существа ──
    fn spawn(&mut self, k: u8, p: V3, group: u32) {
        let (gy, gm) = self.ground(p[0].floor() as i32, p[2].floor() as i32, H as i32);
        if matches!(gm, VOID | LAVA | WATER) { return }
        let (id, r) = (self.next_id, self.rnd());
        self.next_id += 1;
        let col = [0xc83c3c, 0x3c78c8, 0xe0b030, 0x3ca050, 0x9050c0, 0xe07030, 0xe8e8e8][(r % 7) as usize];
        let p = [p[0], gy as f32, p[2]];
        let st = [S_WANDER, S_PATROL, S_EXPLORE][k as usize];
        self.cre.push(Cre { id, k, p, vy: 0., yaw: (r % 628) as f32 / 100., spd: 0., st, tgt: p, food: 0.8, water: 0.8, hp: 1., stam: 1., age: (r % 60) as f32,
            group: if k == 0 && group == 0 { id + 100_000 } else { group }, home: p, inv: 0, built: 0, think: (r % 100) as f32 / 300., ns: 0., alive: true, col, other: 0 });
    }
    /// Опорная поверхность под существом: первый непустой воксель не выше py+3 (кроны над головой не считаются землёй).
    fn ground(&self, x: i32, z: i32, py: i32) -> (i32, u8) {
        if !self.w.loaded(x, z) { return (i32::MAX / 2, VOID) }
        let start = self.w.top(x, z).min(py + 3).min(H as i32 - 1);
        for y in (0..=start).rev() { let m = self.w.v[vi(x, y, z)]; if surf(m) { return (y + 1, m) } }
        (1, AIR)
    }
    fn near_mat(&self, p: V3, m: u8, r: i32) -> bool {
        let (x, z, y) = (p[0].floor() as i32, p[2].floor() as i32, p[1] as i32);
        (-r..=r).any(|dz| (-r..=r).any(|dx| self.ground(x + dx, z + dz, y).1 == m))
    }
    fn find_col(&mut self, p: V3, r: f32, want: u8) -> Option<V3> {
        for _ in 0..20 { let q = self.rand_near(p, r); if self.ground(q[0].floor() as i32, q[2].floor() as i32, p[1] as i32 + 6).1 == want { return Some(q) } }
        None
    }
    fn find_wood(&mut self, p: V3, r: f32) -> Option<V3> {
        for _ in 0..24 {
            let q = self.rand_near(p, r);
            let (x, z) = (q[0].floor() as i32, q[2].floor() as i32);
            if !self.w.loaded(x, z) { continue }
            let t = self.w.top(x, z);
            if (t - 20..=t).any(|y| self.w.get(x, y, z) == WOOD) { return Some([x as f32 + 0.5, p[1], z as f32 + 0.5]) }
        }
        None
    }
    fn danger(&self, p: V3) -> Option<V3> {
        let (x, y, z) = (p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32);
        for dz in [-8, -4, 0, 4, 8] { for dx in [-8, -4, 0, 4, 8] { for dy in [-1, 1, 3] {
            if matches!(self.w.get(x + dx, y + dy, z + dz), FIRE | LAVA) { return Some([(x + dx) as f32, y as f32, (z + dz) as f32]) }
        } } }
        None
    }
    /// Уровни 3–4: ИИ существ. Решения — раз в ~0.3 с (с разнесением по id), движение — каждый тик.
    fn cre_tick(&mut self, dt: f32, lvl: u8, cap: usize) {
        self.stt.ai_ns = [0.; 3];
        self.stt.ai_n = [0; 3];
        if lvl < 3 { return }
        let snap: Vec<(u32, u8, V3, u32)> = self.cre.iter().filter(|c| c.alive).map(|c| (c.id, c.k, c.p, c.group)).collect();
        let mut cres = std::mem::take(&mut self.cre);
        let (mut kills, mut births) = (vec![], vec![]);
        let room = cres.len() < cap;
        for c in cres.iter_mut() {
            if !c.alive || (c.k == 2 && lvl < 4) { continue }
            let t0 = Instant::now();
            c.age += dt;
            c.think -= dt;
            if c.think <= 0. { c.think = 0.25 + (c.id % 10) as f32 * 0.01; self.think(c, &snap, &mut kills, &mut births, room) }
            self.walk(c, dt);
            let ns = t0.elapsed().as_nanos() as f32;
            c.ns = c.ns * 0.9 + ns * 0.1;
            self.stt.ai_ns[c.k as usize] += ns;
            self.stt.ai_n[c.k as usize] += 1;
        }
        for (pr, v) in kills {
            if let Some(c) = cres.iter_mut().find(|c| c.id == v && c.alive) {
                c.alive = false;
                for _ in 0..8 { self.fx.push(([c.p[0], c.p[1] + 1.5, c.p[2]], 0xb02018)) }
                self.ev.push((format!("Хищник #{} поймал травоядное #{}", pr, v), format!("Predator #{} caught herbivore #{}", pr, v)));
            }
        }
        let w = &self.w;
        cres.retain(|c| c.alive && w.loaded(c.p[0].floor() as i32, c.p[2].floor() as i32));
        self.cre = cres;
        for (k, p, g) in births { if self.cre.len() < cap { self.spawn(k, p, g) } }
    }
    fn think(&mut self, c: &mut Cre, snap: &[(u32, u8, V3, u32)], kills: &mut Vec<(u32, u32)>, births: &mut Vec<(u8, V3, u32)>, room: bool) {
        let (cid, cp, k) = (c.id, c.p, c.k as usize);
        let (fx, fy, fz) = (cp[0].floor() as i32, cp[1].floor() as i32, cp[2].floor() as i32);
        c.food = (c.food - 0.3 * [0.008, 0.01, 0.004][k]).max(0.);
        c.water = (c.water - 0.3 * [0.01, 0.008, 0.005][k]).max(0.);
        if c.food <= 0. || c.water <= 0. { c.hp -= 0.015 }
        if matches!(self.w.get(fx, fy, fz), FIRE | LAVA) { c.hp -= 0.3 }
        if c.hp <= 0. {
            c.alive = false;
            self.ev.push((format!("{} #{} погибло", CRE_NAME[k][0], cid), format!("{} #{} died", CRE_NAME[k][1], cid)));
            return;
        }
        let near = |kk: u8, r: f32| snap.iter().filter(|s| s.1 == kk && s.0 != cid).map(|s| (s.0, s.2, dist_h(s.2, cp))).filter(|s| s.2 < r).min_by(|a, b| a.2.total_cmp(&b.2));
        let danger = self.danger(cp);
        match c.k {
            0 => { // травоядное: опасность → хищник → вода → трава → стадо
                if let Some(d) = danger { (c.st, c.tgt, c.spd) = (S_AVOID, away(cp, d, 20.), 8.) }
                else if let Some((pid, pp, _)) = near(1, 30.) { (c.st, c.tgt, c.spd, c.other) = (S_FLEE, away(cp, pp, 30.), 11., pid) }
                else if c.water < 0.35 {
                    if self.near_mat(cp, WATER, 3) { (c.st, c.spd) = (S_DRINK, 0.); c.water = (c.water + 0.12).min(1.) }
                    else { (c.st, c.spd) = (S_SEEKW, 4.); if dist_h(c.tgt, cp) < 2. || self.chance(0.1) { c.tgt = match self.find_col(cp, 60., WATER) { Some(p) => p, None => self.rand_near(cp, 30.) } } }
                } else if c.food < 0.6 {
                    let (gy, gm) = self.ground(fx, fz, fy);
                    if gm == GRASS { (c.st, c.spd) = (S_GRAZE, 0.); c.food = (c.food + 0.07).min(1.); if self.chance(0.06) { self.put(fx, gy - 1, fz, DIRT) } }
                    else { (c.st, c.spd) = (S_GRAZE, 3.); if dist_h(c.tgt, cp) < 1.5 || self.chance(0.1) { c.tgt = match self.find_col(cp, 40., GRASS) { Some(p) => p, None => self.rand_near(cp, 20.) } } }
                } else {
                    let (mut sx, mut sz, mut n) = (0., 0., 0.);
                    for s in snap { if s.1 == 0 && s.3 == c.group && s.0 != cid && dist_h(s.2, cp) < 60. { sx += s.2[0]; sz += s.2[2]; n += 1. } }
                    if n > 0. && (sx / n - cp[0]).hypot(sz / n - cp[2]) > 10. { (c.st, c.spd) = (S_HERD, 4.); c.tgt = self.rand_near([sx / n, cp[1], sz / n], 3.) }
                    else if self.chance(0.3) { (c.st, c.spd) = (S_WANDER, 1.5); c.tgt = self.rand_near(cp, 8.) } else if c.st != S_WANDER { (c.st, c.spd) = (S_REST, 0.) }
                    if room && c.food > 0.7 && c.water > 0.6 && c.age > 40. && self.chance(0.006) { births.push((0, self.rand_near(cp, 2.), c.group)) }
                    let (rx, rz) = (fx + (self.rnd() % 21) as i32 - 10, fz + (self.rnd() % 21) as i32 - 10); // трава медленно отрастает
                    let (gy2, gm2) = self.ground(rx, rz, fy);
                    if gm2 == DIRT && self.chance(0.3) { self.put(rx, gy2 - 1, rz, GRASS) }
                }
            }
            1 => { // хищник: опасность → еда → отдых → охота → патруль территории
                c.stam = (c.stam + if c.st == S_HUNT { -0.04 } else { 0.03 }).clamp(0., 1.);
                if let Some(d) = danger { (c.st, c.tgt, c.spd) = (S_AVOID, away(cp, d, 20.), 8.) }
                else if c.st == S_EAT && c.food < 0.95 { c.spd = 0.; c.food += 0.1 }
                else if c.st == S_REST && c.stam < 0.8 { c.spd = 0. }
                else if c.stam < 0.1 { (c.st, c.spd) = (S_REST, 0.) }
                else if let Some((pid, pp, d)) = if c.food < 0.75 { near(0, 70.) } else { None } {
                    (c.st, c.tgt, c.spd, c.other) = (S_HUNT, pp, 12., pid);
                    if d < 3. { kills.push((cid, pid)); (c.st, c.spd) = (S_EAT, 0.); c.food = (c.food + 0.4).min(1.) }
                } else {
                    (c.st, c.spd) = (S_PATROL, 3.);
                    if dist_h(c.tgt, cp) < 2. || self.chance(0.05) { c.tgt = self.rand_near(c.home, 50.) }
                    if room && c.food > 0.9 && c.age > 90. && self.chance(0.002) { births.push((1, self.rand_near(cp, 2.), 0)) }
                }
                if self.near_mat(cp, WATER, 2) { c.water = 1. }
            }
            _ => { // человек: опасность → рубка → стройка → поиск леса → исследование
                if let Some(d) = danger.or_else(|| near(1, 24.).map(|x| x.1)) { (c.st, c.tgt, c.spd) = (S_AVOID, away(cp, d, 24.), 9.) }
                else if c.st == S_CHOP {
                    let (tx, tz) = (c.tgt[0].floor() as i32, c.tgt[2].floor() as i32);
                    if dist_h(c.tgt, cp) < 3. {
                        c.spd = 0.;
                        let t = self.w.top(tx, tz);
                        match (t - 20..=t).find(|&y| self.w.get(tx, y, tz) == WOOD) {
                            Some(y) => { self.put(tx, y, tz, AIR); c.inv += 1; if c.inv >= 8 { c.st = S_BUILD } }
                            None => c.st = S_EXPLORE,
                        }
                    } else { c.spd = 4. }
                } else if c.inv >= 4 && (c.st == S_BUILD || c.inv >= 8) {
                    (c.st, c.tgt) = (S_BUILD, c.home);
                    if dist_h(c.home, cp) < 7. {
                        c.spd = 0.;
                        let (hx, hz) = (c.home[0].floor() as i32, c.home[2].floor() as i32);
                        let base = self.ground(hx, hz, c.home[1] as i32 + 2).0;
                        let spot = (0..32i32).map(|i| { let j = i % 8 - 4; [(j, -4), (4, j), (-j, 4), (-4, -j)][i as usize / 8] }).filter(|&(dx, dz)| !(dz == 4 && dx.abs() <= 1))
                            .find(|&(dx, dz)| { let (gy, gm) = self.ground(hx + dx, hz + dz, base + 8); gy < base + 6 && gm != WATER && gm != LAVA && gm != VOID });
                        match spot {
                            Some((dx, dz)) => { let gy = self.ground(hx + dx, hz + dz, base + 8).0; self.put(hx + dx, gy, hz + dz, WOOD); c.inv -= 1; c.built += 1;
                                if c.built % 60 == 0 { self.ev.push((format!("Человек #{} достроил хижину", cid), format!("Human #{} finished a hut", cid))) } }
                            None => c.home = self.rand_near(cp, 30.),
                        }
                    } else { c.spd = 4. }
                } else if c.inv < 8 && self.chance(0.15) && let Some(p) = self.find_wood(cp, 40.) { (c.st, c.tgt, c.spd) = (S_CHOP, p, 4.) }
                else if c.st != S_EXPLORE || dist_h(c.tgt, cp) < 2. || self.chance(0.03) {
                    (c.st, c.spd) = (S_EXPLORE, 4.);
                    c.tgt = self.rand_near(cp, 50.);
                    for _ in 0..4 { if self.ground(c.tgt[0].floor() as i32, c.tgt[2].floor() as i32, fy + 6).1 != WATER { break } c.tgt = self.rand_near(cp, 50.) }
                }
                if self.near_mat(cp, LEAF, 4) || self.ground(fx, fz, fy).1 == GRASS { c.food = (c.food + 0.01).min(1.) } // собирательство
                if self.near_mat(cp, WATER, 3) { c.water = 1. }
            }
        }
    }
    fn walk(&mut self, c: &mut Cre, dt: f32) {
        let g = self.pl.g * 2.55 * SF;
        let (dx, dz) = (c.tgt[0] - c.p[0], c.tgt[2] - c.p[2]);
        if c.spd > 0. && dx.hypot(dz) > 0.8 {
            let da = ((dz.atan2(dx) - c.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).clamp(-5. * dt, 5. * dt);
            c.yaw += da;
            let step = c.spd * dt;
            let (nx, nz) = (c.p[0] + c.yaw.cos() * step, c.p[2] + c.yaw.sin() * step);
            let (gy, gm) = self.ground(nx.floor() as i32, nz.floor() as i32, c.p[1] as i32);
            if gy as f32 > c.p[1] + 2.2 || gm == LAVA || gm == VOID || (gm == WATER && c.k == 1) { c.yaw += 1.3; c.tgt = add(c.p, [c.yaw.cos() * 6., 0., c.yaw.sin() * 6.]) }
            else { c.p[0] = nx; c.p[2] = nz }
        }
        let (gy, gm) = self.ground(c.p[0].floor() as i32, c.p[2].floor() as i32, c.p[1] as i32);
        if gm == VOID { return }
        let gy = gy as f32 - if gm == WATER { 1.6 } else { 0. }; // в воде — плывёт, погрузившись по грудь
        if gy >= c.p[1] { c.p[1] = gy; c.vy = 0. } else { c.vy -= g * dt; c.p[1] = (c.p[1] + c.vy * dt).max(gy) }
    }
}

// ─── EDITOR: текстовый запрос мира (RU/EN) и мини-язык правил «если A [рядом B] то C N%» ──────────
const DEFAULT_PROMPT: &str = "острова, горы, лес, жизнь, животные, люди";
const DEFAULT_RULES: [&str; 16] = [
    "если дерево рядом огонь то огонь 20%", "если листва рядом огонь то огонь 35%", "если трава рядом огонь то огонь 25%",
    "если жизнь рядом огонь то огонь 40%", "если огонь то дым 7%", "если дым то воздух 3%", "если вода рядом огонь то пар 50%",
    "если пар то воздух 2%", "если вода рядом лава то пар 20%", "если лава рядом пар то камень 30%", "если песок рядом лава то стекло 2%",
    "если лед рядом огонь то вода 25%", "если снег рядом огонь то вода 35%", "если лед рядом лава то вода 30%",
    "если трава рядом лава то огонь 20%", "если дерево рядом лава то огонь 30%",
];
const FEATS: [(&str, &str, &str); 17] = [
    ("город|дом|здан|деревн|посел|улиц|city|town|house|build|village|street", "города", "cities"),
    ("люд|челов|психол|общест|экономи|социу|цивилиз|people|human|psych|societ|econom|civiliz", "люди", "people"),
    ("гор|скал|холм|утес|mountain|hill|cliff|peak", "горы", "mountains"),
    ("равнин|плоск|поле|степ|луг|flat|plain|field|meadow|steppe", "равнины", "plains"),
    ("лес|дерев|чащ|джунгл|тайг|forest|tree|jungle|wood|taiga", "лес", "forest"),
    ("снег|лед|льд|зим|мороз|арктик|антаркт|ледник|холод|snow|ice|winter|arctic|glacier|cold|frozen", "лёд и снег", "ice & snow"),
    ("пустын|песо|песк|дюн|жар|desert|sand|dune|hot", "пустыня", "desert"),
    ("вулкан|лав|магм|извер|volcan|lava|magma|erupt", "вулканы", "volcanoes"),
    ("жизн|клет|бактер|эволю|зарожд|организм|живн|life|cell|bacteri|evolu|organism|genesis", "клеточная жизнь", "cellular life"),
    ("океан|мор|остров|вод|озер|архипел|ocean|sea|island|water|lake|archipel", "океан и острова", "ocean & islands"),
    ("пещер|подзем|шахт|грот|cave|undergr|mine|grotto", "пещеры", "caves"),
    ("руин|разруш|войн|апокал|катастр|ruin|destr|war|apocal|catastr", "руины", "ruins"),
    ("животн|звер|фаун|олен|волк|стад|хищн|animal|beast|fauna|deer|wolf|herd|predator", "животные", "animals"),
    ("луна|лунн|moon|lunar", "Луна", "Moon"),
    ("марс|mars", "Марс", "Mars"),
    ("астероид|космос|орбит|невесом|пояс|space|asteroid|orbit|belt", "пояс астероидов", "asteroid belt"),
    ("планета земля|земля|earth", "Земля", "Earth"),
];
fn norm(s: &str) -> String { s.to_lowercase().replace('ё', "е") }
fn words(s: &str) -> Vec<String> { norm(s).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect() }
fn stem(list: &str, w: &str) -> bool { list.split('|').any(|s| w.starts_with(s)) }
fn mat_of(w: &str) -> Option<u8> { MATS.iter().position(|m| stem(m.stems, w)).map(|i| i as u8) }
fn parse_rule(s: &str) -> Option<Rule> {
    let (mut ms, mut near, mut p) = (vec![], false, 10.);
    for w in words(s) {
        if let Ok(n) = w.parse::<f32>() { p = n }
        else if ["рядом", "возле", "около", "касается", "near", "touching", "next", "beside"].contains(&w.as_str()) { if ms.len() != 1 { return None } near = true }
        else if let Some(m) = mat_of(&w) { ms.push(m) }
    }
    let p = (p / 100.).clamp(0., 1.);
    match (near, ms.as_slice()) { (true, &[a, b, c]) => Some(Rule { a, b, c, p }), (false, &[a, c]) => Some(Rule { a, b: VOID, c, p }), _ => None }
}
fn rule_txt(r: &Rule, ru: bool) -> String {
    let n = |m: u8| if ru { MATS[m as usize].ru.to_lowercase() } else { MATS[m as usize].en.to_lowercase() };
    let p = (r.p * 100.).round();
    match (r.b == VOID, ru) {
        (true, true) => format!("если {} то {} {}%", n(r.a), n(r.c), p),
        (true, false) => format!("if {} then {} {}%", n(r.a), n(r.c), p),
        (false, true) => format!("если {} рядом {} то {} {}%", n(r.a), n(r.b), n(r.c), p),
        (false, false) => format!("if {} near {} then {} {}%", n(r.a), n(r.b), n(r.c), p),
    }
}
/// Особенности планеты: на Луне, Марсе и в поясе астероидов нет леса, жизни, городов и фауны.
fn planet_defaults(g: &mut Gen) {
    match g.pl { 1 => (g.base, g.amp) = (25., 8.), 2 => (g.base, g.amp) = (28., 14.), _ => {} }
    if g.pl != 0 { (g.forest, g.life, g.city, g.fauna, g.people) = (0., 0., 0., 0., 0.); (g.ruins, g.volcano, g.snow, g.desert) = (false, false, false, false) }
}
/// Запрос на естественном языке → параметры генератора + список распознанных понятий.
fn parse_prompt(s: &str, seed: u64) -> (Gen, Vec<usize>) {
    let mut f = vec![];
    for w in words(s) { if let Some(i) = FEATS.iter().position(|x| stem(x.0, &w)) { if !f.contains(&i) { f.push(i) } } }
    let mut g = Gen::base(seed);
    for i in 0..FEATS.len() {
        if !f.contains(&i) { continue }
        match i {
            0 => g.city = 0.7, 1 => { g.city = g.city.max(0.4); g.people = 1.5 } 2 => { g.amp = 26.; g.base += 5. } 3 => g.amp = 3., 4 => g.forest = 1.,
            5 => g.snow = true, 6 => { g.desert = true; g.forest *= 0.15; g.sea = 10. } 7 => g.volcano = true, 8 => g.life = 0.9,
            9 => { g.sea = 30.; g.base = g.base.min(26.) } 10 => g.caves = true, 11 => { g.ruins = true; g.city = g.city.max(0.5) }
            12 => g.fauna = 2., 13 => g.pl = 1, 14 => g.pl = 2, 15 => g.pl = 3, _ => g.pl = 0,
        }
    }
    planet_defaults(&mut g);
    (g, f)
}

// ─── RENDER: текст (сверху), 2D (срез), 3D (рейкаст полосами на всех ядрах, пропуск пустых блоков 4³), ─
//             составные модели существ, небо планет и космос
type V3 = [f32; 3];
const FOV: f32 = 0.7; const SUN: V3 = [0.4, 0.8, 0.45]; const MC: f32 = 0.5; // ячейка модели существа = 0.5 вокселя = 0.25 м
fn add(a: V3, b: V3) -> V3 { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
fn sub(a: V3, b: V3) -> V3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn sc(a: V3, k: f32) -> V3 { [a[0] * k, a[1] * k, a[2] * k] }
fn dot(a: V3, b: V3) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn nrm(a: V3) -> V3 { sc(a, 1. / dot(a, a).sqrt()) }
fn basis(yaw: f32, pitch: f32) -> (V3, V3, V3) {
    let (cy, sy, cp, sp) = (yaw.cos(), yaw.sin(), pitch.cos(), pitch.sin());
    let (f, r) = ([cp * cy, sp, cp * sy], [-sy, 0., cy]);
    (f, r, [r[1] * f[2] - r[2] * f[1], r[2] * f[0] - r[0] * f[2], r[0] * f[1] - r[1] * f[0]])
}
#[derive(Clone, Copy)]
struct View { p: V3, yaw: f32, pitch: f32 }
#[derive(Clone, Copy)]
struct Hit { p: [i32; 3], prev: [i32; 3], m: u8, t: f32, ax: usize, s: i32 }
/// Окружение кадра: планета и видимость атмосферы (1 — у поверхности, 0 — в космосе).
#[derive(Clone, Copy)]
struct Env { pl: Planet, pid: u8, air: f32 }
/// Луч по вокселям (Amanatides–Woo) с пропуском однородных макро-блоков 4³ пустоты (или вещества pass)
/// и прыжком сквозь пустое пространство над миром. pass — вещество, сквозь которое луч проходит как сквозь воздух.
fn dda(w: &World, o: V3, d: V3, maxd: f32, pass: u8, rs: &mut RStats) -> Option<Hit> {
    let inv = [1. / d[0], 1. / d[1], 1. / d[2]];
    let mut p = [o[0].floor() as i32, o[1].floor() as i32, o[2].floor() as i32];
    let st = [d[0].signum() as i32 * (d[0] != 0.) as i32, d[1].signum() as i32 * (d[1] != 0.) as i32, d[2].signum() as i32 * (d[2] != 0.) as i32];
    let td = [inv[0].abs(), inv[1].abs(), inv[2].abs()];
    let next = |p: [i32; 3], a: usize| if st[a] == 0 { f32::INFINITY } else { ((p[a] + (st[a] > 0) as i32) as f32 - o[a]) * inv[a] };
    let mut tm = [next(p, 0), next(p, 1), next(p, 2)];
    let (mut t, mut ax, mut prev) = (0f32, 1usize, p);
    loop {
        if t > maxd { return None }
        if p[1] >= H as i32 {
            if d[1] >= 0. { return None }
            let te = (H as f32 - o[1]) * inv[1]; // прыжок к верхней границе мира
            if te > t {
                rs.skips += 1;
                t = te;
                p = [(o[0] + d[0] * te).floor() as i32, H as i32 - 1, (o[2] + d[2] * te).floor() as i32];
                (prev, ax) = ([p[0], H as i32, p[2]], 1);
                tm = [next(p, 0), next(p, 1), next(p, 2)];
                continue;
            }
        } else if p[1] < 0 { return Some(Hit { p, prev, m: BEDROCK, t, ax, s: st[ax] }) }
        else if !w.loaded(p[0], p[2]) { return Some(Hit { p, prev, m: VOID, t, ax, s: st[ax] }) }
        else {
            let mb = w.mac[mi(p[0], p[1], p[2])];
            let size = if w.sup[sec(p[0], p[1], p[2])] == AIR { 16 } else if mb == AIR || (mb == pass && pass != AIR) { 4 } else { 0 };
            if size > 0 { // однородный куб (секция 16^3 или блок 4^3): выход из него одним шагом
                let b0 = [p[0] & !(size - 1), p[1] & !(size - 1), p[2] & !(size - 1)];
                let (mut te, mut ea) = (f32::INFINITY, 0);
                for a in 0..3 { if st[a] != 0 { let tt = ((if st[a] > 0 { b0[a] + size } else { b0[a] }) as f32 - o[a]) * inv[a]; if tt < te { te = tt; ea = a } } }
                if size == 16 { rs.big += 1 } else { rs.skips += 1 }
                for a in 0..3 {
                    if a == ea { prev[a] = if st[a] > 0 { b0[a] + size - 1 } else { b0[a] }; p[a] = prev[a] + st[a] }
                    else { p[a] = ((o[a] + d[a] * te).floor() as i32).clamp(b0[a], b0[a] + size - 1); prev[a] = p[a] }
                }
                (t, ax) = (te.max(t), ea);
                tm = [next(p, 0), next(p, 1), next(p, 2)];
                continue;
            }
            let m = w.v[vi(p[0], p[1], p[2])];
            if m != AIR && m != pass { return Some(Hit { p, prev, m, t, ax, s: st[ax] }) }
            rs.steps += 1;
        }
        prev = p;
        ax = if tm[0] < tm[1] { if tm[0] < tm[2] { 0 } else { 2 } } else if tm[1] < tm[2] { 1 } else { 2 };
        t = tm[ax];
        tm[ax] += td[ax];
        p[ax] += st[ax];
    }
}
fn stars(d: V3) -> u32 {
    let h = hf((d[0] * 260.).floor() as i32, (d[1] * 260.).floor() as i32, (d[2] * 260.).floor() as i32, 991);
    if h < 0.003 { mul(0xffffff, 0.35 + h / 0.003 * 0.65) } else { 0x02030a }
}
/// Поверхность планеты вдали / с орбиты: материки, океаны и облака (Земля), кратерный реголит (Луна), ржавые равнины (Марс).
fn far_ground(pid: u8, x: f32, z: f32) -> u32 {
    let n = noise(x / 900., z / 900., 0., 404) * 0.7 + noise(x / 230., z / 230., 0., 405) * 0.3;
    match pid {
        0 => {
            let base = if n < 0.5 { mix(0x1d3f7a, 0x2c64a8, n * 2.) } else { mix(0x4f7a3c, 0x8a7a50, (n - 0.5) * 2.) };
            let cl = noise(x / 400., z / 400., 0., 406);
            if cl > 0.6 { mix(base, 0xf4f6f8, ((cl - 0.6) * 4.).min(0.9)) } else { base }
        }
        1 => mix(0x5a5a58, 0xb0b0aa, n),
        2 => mix(0x7a3a22, 0xc8784a, n),
        _ => 0x02030a,
    }
}
fn sky(d: V3, o: V3, e: &Env) -> u32 {
    let pl = &e.pl;
    if d[1] < -0.02 && !pl.float { // ниже горизонта — даль планеты (с высоты видна как с орбиты)
        let tt = o[1].max(1.) / -d[1];
        let g = far_ground(e.pid, o[0] + d[0] * tt, o[2] + d[2] * tt);
        return mix(g, pl.sky0, (e.air * (1. - (-tt / 900.).exp()) + 0.15 * e.air).min(1.));
    }
    let atmo = mix(pl.sky0, pl.sky1, d[1].max(0.).sqrt());
    let mut c = if e.air >= 0.99 { atmo } else { mix(stars(d), atmo, e.air) };
    let s = dot(d, SUN);
    if s > 0.9994 { c = 0xfffbe8 } else if s > 0.97 { c = mix(c, 0xfff2c8, (s - 0.97) / 0.03 * (0.25 + 0.4 * (1. - e.air))) }
    let bd = nrm([-0.45, 0.42, 0.78]); // небесное тело: Луна с Земли, Земля с Луны и из космоса, Фобос с Марса
    let (rad, cb) = match e.pid { 0 => (0.02, 0), 2 => (0.012, 2), _ => (0.13, 1) };
    let cr = dot(d, bd);
    if cr > (1. - rad * rad / 2.) {
        let (e1, e2) = (nrm([bd[2], 0., -bd[0]]), [0., 1., 0.]);
        let (u, v) = (dot(d, e1) / rad, dot(sub(d, sc(bd, cr)), e2) / rad);
        let body = match cb { 1 => far_ground(0, u * 9000., v * 9000.), 2 => 0x8a7a6a, _ => mix(0xd8d8d0, 0x9a9a94, noise(u * 4., v * 4., 0., 7)) };
        let lit = (0.35 + 0.65 * (1. - (u * u + v * v)).max(0.).sqrt()).min(1.);
        c = mix(c, mul(body, lit), if e.pid == 0 { 0.55 } else { 1. });
    }
    c
}
fn tex(c: u32, x: i32, y: i32, z: i32) -> u32 { mul(c, 0.88 + hf(x, y, z, 77) * 0.24) }
fn shade(w: &World, o: V3, d: V3, q: &Q, e: &Env, pass0: u8, rs: &mut RStats) -> (u32, f32) {
    let Some(mut h) = dda(w, o, d, q.view, pass0, rs) else { return (sky(d, o, e), 1e9) };
    if h.m == BEDROCK && h.p[1] < 0 && e.pl.float { return (sky(d, o, e), 1e9) } // под астероидами — космос
    let mut tint = None;
    if q.water && matches!(h.m, WATER | GLASS | ICE | STEAM | SMOKE) {
        let tt = (MATS[h.m as usize].col, if h.m == WATER { 0.6 } else { 0.4 });
        tint = Some(tt);
        match dda(w, add(o, sc(d, h.t + 0.01)), d, q.view - h.t, h.m, rs) { Some(h2) => h = Hit { t: h.t + h2.t + 0.01, ..h2 }, None => return (mix(sky(d, o, e), tt.0, tt.1), h.t) }
    }
    let mut c = if h.m == VOID { mix(sky(d, o, e), e.pl.sky0, 0.5) } else { MATS[h.m as usize].col };
    if h.m != VOID {
        if q.tex { c = tex(c, h.p[0], h.p[1], h.p[2]) }
        let mut l = [0.78, if h.s < 0 { 1. } else { 0.5 }, 0.64][h.ax];
        if glow(h.m) { l = 1.2 } else if q.shadows {
            let mut sp = add(o, sc(d, h.t));
            sp[h.ax] -= h.s as f32 * 0.02;
            if dda(w, sp, SUN, 96., AIR, rs).is_some_and(|s| s.m != VOID) { l *= if e.pl.atm > 0.5 { 0.55 } else { 0.25 } } // в вакууме тени чернее
        }
        c = mul(c, l);
        c = mix(c, sky(d, o, e), ((h.t / q.view - 0.55) / 0.45).clamp(0., 1.));
    }
    if let Some((tc, a)) = tint { c = mix(c, tc, a) }
    (c, h.t)
}
/// Число логических ядер (один раз за запуск).
fn cores() -> usize { static N: OnceLock<usize> = OnceLock::new(); *N.get_or_init(|| std::thread::available_parallelism().map_or(4, |n| n.get())) }
/// Параллельный рендер: кадр режется на 4×(ядра) горизонтальных полос, полосы раздаются потокам по кругу
/// (небо и земля равномерно делятся между ядрами). Без Mutex: каждый поток получает свои &mut срезы буфера.
/// Возвращает по каждому потоку: время (мс), статистику лучей и число пикселей.
fn par(buf: &mut [u32], zb: &mut [f32], ww: usize, f: &(dyn Fn(usize, usize, &mut RStats) -> (u32, f32) + Sync)) -> Vec<(f32, RStats, usize)> {
    let n = cores();
    let rows = (buf.len() / ww).div_ceil(n * 4).max(1) * ww;
    let mut jobs: Vec<Vec<(usize, &mut [u32], &mut [f32])>> = (0..n).map(|_| Vec::new()).collect();
    for (i, (b, z)) in buf.chunks_mut(rows).zip(zb.chunks_mut(rows)).enumerate() { jobs[i % n].push((i * rows, b, z)) }
    std::thread::scope(|s| {
        let hs: Vec<_> = jobs.into_iter().map(|job| s.spawn(move || {
            let (t, mut rs, mut px) = (Instant::now(), RStats::default(), 0);
            for (off, b, z) in job { for i in 0..b.len() { let g = off + i; (b[i], z[i]) = f(g % ww, g / ww, &mut rs) } px += b.len() }
            (ms(t), rs, px)
        })).collect();
        hs.into_iter().map(|h| h.join().unwrap_or((0., RStats::default(), 0))).collect()
    })
}

// составные модели существ: сетка ячеек 0.25 м, ось X — «вперёд». Цвет 1 — подставляется цвет одежды/окраса особи
struct Model { sx: i32, sy: i32, sz: i32, c: Vec<u32> }
fn mk_model(sx: i32, sy: i32, sz: i32, boxes: &[(i32, i32, i32, i32, i32, i32, u32)]) -> Model {
    let mut c = vec![0; (sx * sy * sz) as usize];
    for &(x0, y0, z0, x1, y1, z1, col) in boxes { for y in y0..=y1 { for z in z0..=z1 { for x in x0..=x1 { c[((y * sz + z) * sx + x) as usize] = col } } } }
    Model { sx, sy, sz, c }
}
fn model(k: u8) -> &'static Model {
    static M: OnceLock<[Model; 3]> = OnceLock::new();
    &M.get_or_init(|| {
        let (fur, dark, hoof, cream) = (0xa8773e, 0x6a4a28, 0x2a1d10, 0xeadcc4);
        let deer = mk_model(10, 11, 3, &[
            (2, 1, 0, 2, 3, 0, dark), (2, 1, 2, 2, 3, 2, dark), (7, 1, 0, 7, 3, 0, dark), (7, 1, 2, 7, 3, 2, dark),       // ноги
            (2, 0, 0, 2, 0, 0, hoof), (2, 0, 2, 2, 0, 2, hoof), (7, 0, 0, 7, 0, 0, hoof), (7, 0, 2, 7, 0, 2, hoof),       // копыта
            (1, 4, 0, 7, 6, 2, fur), (2, 4, 1, 6, 4, 1, cream), (0, 5, 1, 0, 6, 1, cream),                                // туловище, брюхо, хвост
            (7, 6, 1, 8, 7, 1, fur), (8, 7, 0, 9, 8, 2, 0x9a6a35), (9, 7, 1, 9, 7, 1, hoof), (9, 8, 0, 9, 8, 0, 0x101010), (9, 8, 2, 9, 8, 2, 0x101010), // шея, голова, нос, глаза
            (8, 9, 0, 8, 9, 0, 0x9a6a35), (8, 9, 2, 8, 9, 2, 0x9a6a35), (6, 10, 0, 8, 10, 0, 0xdcc8a0), (6, 10, 2, 8, 10, 2, 0xdcc8a0), // уши, рога
        ]);
        let (grey, dk, belly) = (0x6e7074, 0x4a4c50, 0xb0b0a8);
        let wolf = mk_model(10, 8, 3, &[
            (2, 0, 0, 2, 2, 0, dk), (2, 0, 2, 2, 2, 2, dk), (6, 0, 0, 6, 2, 0, dk), (6, 0, 2, 6, 2, 2, dk),               // лапы
            (1, 3, 0, 6, 5, 2, grey), (1, 5, 0, 6, 5, 2, dk), (2, 3, 1, 5, 3, 1, belly), (0, 4, 1, 0, 5, 1, dk), (0, 3, 1, 0, 3, 1, grey), // туловище, спина, брюхо, хвост
            (7, 4, 0, 8, 6, 2, grey), (9, 4, 1, 9, 5, 1, 0x8a8a86), (9, 5, 1, 9, 5, 1, 0x101010),                       // голова, морда, нос
            (8, 6, 0, 8, 6, 0, 0xffd040), (8, 6, 2, 8, 6, 2, 0xffd040), (7, 7, 0, 7, 7, 0, dk), (7, 7, 2, 7, 7, 2, dk),     // глаза, уши
        ]);
        let skin = 0xf0c8a0;
        let human = mk_model(3, 8, 4, &[
            (1, 0, 1, 1, 0, 2, 0x2a2a2a), (1, 1, 1, 1, 2, 1, 0x34508c), (1, 1, 2, 1, 2, 2, 0x34508c),                     // обувь, ноги
            (0, 3, 1, 2, 5, 2, 1), (1, 3, 0, 1, 5, 0, 1), (1, 3, 3, 1, 5, 3, 1), (1, 2, 0, 1, 2, 0, skin), (1, 2, 3, 1, 2, 3, skin), // торс, рукава, кисти
            (0, 6, 1, 2, 7, 2, skin), (0, 7, 1, 1, 7, 2, 0x4a3020), (0, 6, 1, 0, 6, 2, 0x4a3020), (2, 6, 1, 2, 6, 1, 0x202020), (2, 6, 2, 2, 6, 2, 0x202020), // голова, волосы, глаза
        ]);
        [deer, wolf, human]
    })[k as usize]
}
/// Луч против модели существа: переход в локальные координаты (поворот на yaw), отсечение AABB, DDA по ячейкам.
/// Возвращает (дистанцию в вокселях мира, цвет ячейки, ось грани, знак шага).
fn ray_model(m: &Model, cp: V3, yaw: f32, o: V3, d: V3) -> Option<(f32, u32, usize, i32)> {
    let (cy, sy) = (yaw.cos(), yaw.sin());
    let rel = sub(o, cp);
    let lo = [(rel[0] * cy + rel[2] * sy) / MC + m.sx as f32 / 2., rel[1] / MC, (-rel[0] * sy + rel[2] * cy) / MC + m.sz as f32 / 2.];
    let ld = [(d[0] * cy + d[2] * sy) / MC, d[1] / MC, (-d[0] * sy + d[2] * cy) / MC];
    let dims = [m.sx, m.sy, m.sz];
    let (mut t0, mut t1, mut ax) = (0f32, f32::INFINITY, 1usize);
    for a in 0..3 {
        if ld[a].abs() < 1e-9 { if lo[a] < 0. || lo[a] > dims[a] as f32 { return None } continue }
        let (mut a0, mut a1) = (-lo[a] / ld[a], (dims[a] as f32 - lo[a]) / ld[a]);
        if a0 > a1 { std::mem::swap(&mut a0, &mut a1) }
        if a0 > t0 { (t0, ax) = (a0, a) }
        t1 = t1.min(a1);
    }
    if t0 > t1 { return None }
    let s0 = add(lo, sc(ld, t0 + 1e-4));
    let mut p = [0i32; 3];
    let (mut st, mut tm, mut td) = ([0i32; 3], [f32::INFINITY; 3], [f32::INFINITY; 3]);
    for a in 0..3 {
        p[a] = (s0[a].floor() as i32).clamp(0, dims[a] - 1);
        if ld[a] > 0. { st[a] = 1; td[a] = 1. / ld[a]; tm[a] = t0 + ((p[a] + 1) as f32 - s0[a]) / ld[a] }
        else if ld[a] < 0. { st[a] = -1; td[a] = -1. / ld[a]; tm[a] = t0 + (p[a] as f32 - s0[a]) / ld[a] }
    }
    let mut t = t0;
    for _ in 0..40 {
        if (0..3).any(|a| p[a] < 0 || p[a] >= dims[a]) { return None }
        let c = m.c[((p[1] * m.sz + p[2]) * m.sx + p[0]) as usize];
        if c != 0 { return Some((t, c, ax, st[ax])) }
        ax = if tm[0] < tm[1] { if tm[0] < tm[2] { 0 } else { 2 } } else if tm[1] < tm[2] { 1 } else { 2 };
        t = tm[ax];
        tm[ax] += td[ax];
        p[ax] += st[ax];
    }
    None
}
/// Видимое существо для рендера: прямоугольник на экране отсекает пиксели, которые вообще стоит проверять.
#[derive(Clone, Copy)]
struct VisCre { p: V3, yaw: f32, k: u8, col: u32, tint: f32, r: (i32, i32, i32, i32) }
fn cre_color(vc: &VisCre, c: u32) -> u32 { if c == 1 { vc.col } else if vc.k < 2 { mul(c, vc.tint) } else { c } }
#[allow(clippy::too_many_arguments)]
fn render3d(w: &World, v: &View, q: &Q, e: &Env, vis: &[VisCre], buf: &mut [u32], zb: &mut [f32], ww: usize, wh: usize) -> Vec<(f32, RStats, usize)> {
    let (f, r, u) = basis(v.yaw, v.pitch);
    let asp = ww as f32 / wh as f32;
    let m0 = w.get(v.p[0].floor() as i32, v.p[1].floor() as i32, v.p[2].floor() as i32); // камера под водой / в дыму
    let p0 = if matches!(kind(m0), K::Liquid | K::Gas) { m0 } else { AIR };
    par(buf, zb, ww, &|x, y, rs| {
        let (sx, sy) = ((2. * (x as f32 + 0.5) / ww as f32 - 1.) * FOV * asp, (1. - 2. * (y as f32 + 0.5) / wh as f32) * FOV);
        let d = nrm(add(f, add(sc(r, sx), sc(u, sy))));
        let (mut c, mut t) = shade(w, v.p, d, q, e, p0, rs);
        let (xi, yi) = (x as i32, y as i32);
        for vc in vis {
            if xi < vc.r.0 || xi > vc.r.2 || yi < vc.r.1 || yi > vc.r.3 { continue }
            if let Some((tc, col, ax, s)) = ray_model(model(vc.k), vc.p, vc.yaw, v.p, d) {
                if tc < t {
                    let l = [0.82, if s < 0 { 1. } else { 0.55 }, 0.68][ax];
                    c = mix(mul(cre_color(vc, col), l), sky(d, v.p, e), ((tc / q.view - 0.55) / 0.45).clamp(0., 1.));
                    t = tc;
                }
            }
        }
        (if p0 == AIR { c } else { mix(c, MATS[p0 as usize].col, 0.45) }, t)
    })
}
#[allow(clippy::too_many_arguments)]
fn render2d(w: &World, v: &View, q: &Q, e: &Env, buf: &mut [u32], zb: &mut [f32], ww: usize, wh: usize, ppv: f32) -> Vec<(f32, RStats, usize)> {
    let z = v.p[2].floor() as i32;
    par(buf, zb, ww, &|px, py, _| {
        let (fx, fy) = (v.p[0] + (px as f32 - ww as f32 / 2.) / ppv, v.p[1] + (wh as f32 / 2. - py as f32) / ppv);
        let (x, y) = (fx.floor() as i32, fy.floor() as i32);
        let m = w.get(x, y, z);
        let c = if m == AIR { // фон: глубина за срезом (детализация ГП), иначе небо/космос/пещера
            let mut c = if y > w.top(x, z) || !w.loaded(x, z) {
                if e.air > 0.01 { mix(e.pl.sky0, e.pl.sky1, (y as f32 / H as f32).clamp(0., 1.)) } else { stars([px as f32 * 0.01, py as f32 * 0.013, 0.37]) }
            } else { 0x18161a };
            for k in 1..=q.depth2d { let b = w.get(x, y, z + k); if b != AIR { if b != VOID && kind(b) != K::Gas { c = mul(MATS[b as usize].col, 0.5 - k as f32 * 0.015) } break } }
            c
        } else if m == VOID { 0x101014 } else {
            let mut c = MATS[m as usize].col;
            if q.tex { c = tex(c, x, y, z) }
            if !glow(m) && q.shadows { let dd = w.top(x, z) - y; if dd > 0 { c = mul(c, (1. - dd as f32 * 0.035).max(0.35)) } }
            if q.tex && ppv >= 3. && ((fx - x as f32) * ppv < 1. || (fy - y as f32) * ppv < 1.) { c = mul(c, 0.85) }
            c
        };
        (c, 0.)
    })
}
/// Существо в 2D-срезе: боковая проекция модели (первая непустая ячейка поперёк), разворот по направлению взгляда.
#[allow(clippy::too_many_arguments)]
fn draw_cre2d(buf: &mut [u32], ww: usize, wh: usize, ppv: f32, v: &View, p: V3, yaw: f32, k: u8, col: u32) {
    let m = model(k);
    let dir = if yaw.cos() >= 0. { 1. } else { -1. };
    let cs = (MC * ppv).max(1.);
    for y in 0..m.sy { for x in 0..m.sx {
        let Some(c) = (0..m.sz).map(|z| m.c[((y * m.sz + z) * m.sx + x) as usize]).find(|&c| c != 0) else { continue };
        let c = if c == 1 { col } else { c };
        let (wx, wy) = (p[0] + (x as f32 - m.sx as f32 / 2. + 0.5) * MC * dir, p[1] + (y as f32 + 0.5) * MC);
        let (sx, sy) = (ww as f32 / 2. + (wx - v.p[0]) * ppv - cs / 2., wh as f32 / 2. - (wy - v.p[1]) * ppv - cs / 2.);
        for yy in sy.max(0.) as usize..((sy + cs) as usize).min(wh) { for xx in sx.max(0.) as usize..((sx + cs) as usize).min(ww) { buf[yy * ww + xx] = c } }
    } }
}
fn render_text(cv: &mut Canvas, w: &World, v: &View, q: &Q, me: bool, cres: &[Cre], lvl: u8) {
    let s = q.txt;
    let (cw, chh, vpc) = (6 * s, 11 * s, q.vpc);
    let (cols, rows) = (cv.w / cw, (cv.h - TOP - 16) / chh);
    let (px, pz) = (v.p[0].floor() as i32, v.p[2].floor() as i32);
    for r in 0..rows { for c in 0..cols {
        let (x, z) = (px + (c - cols / 2) * vpc, pz + (r - rows / 2) * vpc);
        if !w.loaded(x, z) { cv.ch(c * cw, TOP + r * chh, '?', 0x303438, s); continue }
        let t = w.top(x, z);
        let a = w.get(x, t + 1, z);
        let m = if matches!(kind(a), K::Fire | K::Gas) { a } else { w.get(x, t, z) };
        let mut col = MATS[m as usize].col;
        if q.mono { col = if m == LIFE { 0xd0ffd0 } else { mul(0x33ff66, 0.15 + t as f32 / H as f32 * 0.4) } } else if q.tex { col = mul(col, 0.55 + t as f32 / H as f32 * 0.8) }
        cv.ch(c * cw, TOP + r * chh, MATS[m as usize].sym, col, s);
    } }
    for k in cres.iter().filter(|k| k.alive && (k.k < 2 || lvl >= 4) && lvl >= 3) { // существа буквами: Т — травоядное, Х — хищник, Ч — человек
        let (c, r) = ((k.p[0].floor() as i32 - px).div_euclid(vpc) + cols / 2, (k.p[2].floor() as i32 - pz).div_euclid(vpc) + rows / 2);
        if c < 0 || r < 0 || c >= cols || r >= rows { continue }
        cv.rect(c * cw, TOP + r * chh, cw, chh, 0x000000, 255);
        cv.ch(c * cw, TOP + r * chh, ['Т', 'Х', 'Ч'][k.k as usize], [0xe0b070, 0xff6050, k.col][k.k as usize], s);
    }
    cv.ch(cols / 2 * cw, TOP + rows / 2 * chh, if me { '@' } else { '+' }, 0xffffff, s);
}
fn upscale(cv: &mut Canvas, wb: &[u32], ww: usize, wh: usize) {
    let (cw, ch) = (cv.w as usize, cv.h as usize);
    let xm: Vec<usize> = (0..cw).map(|x| x * ww / cw).collect();
    for y in 0..ch {
        let s = &wb[y * wh / ch * ww..][..ww];
        for (x, p) in cv.px[y * cw..(y + 1) * cw].iter_mut().enumerate() { *p = s[xm[x]] }
    }
}
/// Счётчик тактов процессора (TSC) для профайлера; на не-x86 возвращает 0.
#[allow(unused_unsafe)]
fn tsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    { unsafe { core::arch::x86_64::_rdtsc() } }
    #[cfg(not(target_arch = "x86_64"))]
    { 0 }
}

// ─── GAME: состояние, меню, ввод, игрок-бог, инструменты, инспектор, профайлер, бюджет кадра ─────
#[derive(Clone, Copy, PartialEq)]
enum Scr { Menu, Settings, Play }
#[derive(Clone, Copy)]
enum Insp { Vox([i32; 3]), Cre(u32) }
struct Part { p: V3, v: V3, c: u32, t: f32 }
pub struct Game {
    scr: Scr, started: bool, ru: bool, pub quit: bool, help: bool, editor: bool, line: String, out: Vec<String>, prompt: String,
    sl: [i32; 6], sel: usize, q: Q, menu_sel: usize, drag: Option<usize>, prof: bool, insp: Option<(Insp, i32, i32)>,
    sim: Sim, wgen: Gen, regen: u64, v: View, vel: V3, me: bool, ground: bool, face: f32, tool: usize, mat: u8, brush: i32, paused: bool,
    pub cv: Canvas, sw: i32, sh: i32, wb: Vec<u32>, zb: Vec<f32>, ww: usize, wh: usize, parts: Vec<Part>,
    log: VecDeque<(String, String, f32)>, hover: Option<Hit>, hover_cre: Option<u32>, mouse: (i32, i32),
    ms: [f32; 6], frame: f32, fps: f32, budget: f32, last: Instant, acc: f32, acc_life: f32, lag: f32, bott: Option<usize>, glitch: f32,
    spiral: Vec<(i32, i32)>, rng: u64, time: f32, cd: f32, pending: usize, placed: bool,
    thr: Vec<(f32, RStats, usize)>, ai_ms: f32, mac_n: usize, mac_ms: f32, present: f32, cpms: f32, tsc0: u64, gen_n: usize, homog: f32,
}
fn ms(t: Instant) -> f32 { t.elapsed().as_secs_f32() * 1000. }
impl Game {
    pub fn new(sw: i32, sh: i32) -> Game {
        let s = (sh / 540).max(1);
        let sl = [50, 80, 40, 40, 25, 80];
        let mut spiral: Vec<(i32, i32)> = (-7..=7).flat_map(|z| (-7..=7).map(move |x| (x, z))).collect();
        spiral.sort_by_key(|&(x, z)| x * x + z * z);
        Game {
            scr: Scr::Menu, started: false, ru: true, quit: false, help: false, editor: false, line: String::new(), out: vec![], prompt: DEFAULT_PROMPT.into(),
            sl, sel: 0, q: quality(&sl), menu_sel: 0, drag: None, prof: false, insp: None,
            sim: Sim::new(), wgen: parse_prompt(DEFAULT_PROMPT, 1).0, regen: 1, v: View { p: [16., 124., 16.], yaw: 0.6, pitch: -0.45 }, vel: [0.; 3],
            me: false, ground: false, face: 1., tool: 0, mat: STONE, brush: 4, paused: false,
            cv: Canvas::new(sw / s, sh / s), sw, sh, wb: vec![], zb: vec![], ww: 0, wh: 0, parts: vec![],
            log: VecDeque::new(), hover: None, hover_cre: None, mouse: (0, 0),
            ms: [0.; 6], frame: 0., fps: 60., budget: 16.7, last: Instant::now(), acc: 0., acc_life: 0., lag: 0., bott: None, glitch: 0.,
            spiral, rng: 0x9E37_79B9_7F4A_7C15, time: 0., cd: 0., pending: 0, placed: false,
            thr: vec![], ai_ms: 0., mac_n: 0, mac_ms: 0., present: 0., cpms: 0., tsc0: 0, gen_n: 0, homog: 0.,
        }
    }
    fn l(&self, ru: &'static str, en: &'static str) -> &'static str { if self.ru { ru } else { en } }
    fn say(&mut self, ru: String, en: String) { self.log.push_back((ru, en, self.time)); while self.log.len() > self.q.hist { self.log.pop_front(); } }
    fn rf(&mut self) -> f32 { self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17; (self.rng >> 40) as f32 / 16_777_216. }
    pub fn wants_look(&self) -> bool { self.scr == Scr::Play && self.q.mode == 2 && !self.editor }
    fn resize(&mut self, w: i32, h: i32) { if w <= 0 || h <= 0 { return } (self.sw, self.sh) = (w, h); let s = (h / 540).max(1); self.cv = Canvas::new(w / s, h / s) }
    fn env(&self) -> Env {
        let pl = self.sim.pl;
        let alt = ((self.v.p[1] - H as f32) / 700.).clamp(0., 1.); // выше мира атмосфера редеет: небо чернеет, проступают звёзды
        Env { pl, pid: self.wgen.pl, air: if pl.atm > 0. { 1. - alt } else { 0. } }
    }
    fn cre_active(&self, c: &Cre) -> bool { c.alive && self.q.level >= 3 && (c.k < 2 || self.q.level >= 4) }

    /// Один кадр: ввод → накопитель (чанки) → ОЗУ (жизнь) → ЦП (физика, ИИ) → ГП (рендер) → Мир (сборка кадра).
    pub fn frame(&mut self, inp: &mut Input) {
        let now = Instant::now();
        self.tsc0 = tsc();
        let dt = now.duration_since(self.last).as_secs_f32().clamp(1e-4, 0.1);
        (self.last, self.time) = (now, self.time + dt);
        self.fps += (1. / dt - self.fps) * 0.1;
        if let Some((w, h)) = inp.size { self.resize(w, h) }
        self.q = quality(&self.sl);
        self.budget = 1000. / self.q.fps;
        self.mouse = (inp.mx * self.cv.w / self.sw.max(1), inp.my * self.cv.h / self.sh.max(1));
        self.input(inp, dt);
        let t = Instant::now();
        self.stream();
        let ssd = ms(t);
        if !self.placed && self.pending == 0 { self.place() }
        let (mut ram, mut cpu) = (0., 0.);
        if !self.paused && (self.scr == Scr::Play || !self.started) {
            let (hz, c) = (self.q.tick_hz, (self.v.p[0].floor() as i32, self.v.p[2].floor() as i32));
            let step = 1. / hz;
            self.acc += dt;
            let mut n = 0;
            while self.acc >= step && n < 4 {
                (self.acc, self.acc_life, n) = (self.acc - step, self.acc_life + step, n + 1);
                let t = Instant::now();
                if self.acc_life >= 1. / hz.min(10.) { self.acc_life = 0.; self.sim.life_tick(c, self.q.radius) }
                ram += ms(t);
                if self.q.level >= 1 { let t = Instant::now(); self.sim.phys_tick(c, &self.q); cpu += ms(t) }
                let t = Instant::now();
                self.sim.cre_tick(step, self.q.level, self.q.max_ent);
                let a = ms(t);
                cpu += a;
                self.ai_ms = a;
            }
            if self.acc > step * 4. { self.acc = 0.; self.lag = 1. }
        }
        self.lag = (self.lag - dt).max(0.);
        for (r, e) in std::mem::take(&mut self.sim.ev) { self.say(r, e) }
        for (p, c) in std::mem::take(&mut self.sim.fx) {
            if self.parts.len() >= self.q.parts { break }
            let v = [(self.rf() - 0.5) * 28., self.rf() * 20. + 6., (self.rf() - 0.5) * 28.];
            let t = 1. + self.rf() * 1.5;
            self.parts.push(Part { p, v, c, t })
        }
        let t = Instant::now();
        let tm = Instant::now(); // поддержка макро-сетки 4³ (пропуск пустоты) — часть затрат «видеокарты»
        self.mac_n = self.sim.w.recheck(8192);
        self.mac_ms = ms(tm);
        if self.prof && (self.time * 2.) as i32 % 2 == 0 { let m = &self.sim.w.mac; self.homog = m.iter().step_by(7).filter(|&&b| b != MIXED).count() as f32 * 7. / m.len() as f32 }
        self.update_parts(dt);
        self.render_world();
        let gpu = ms(t);
        let t = Instant::now();
        self.compose();
        let ui = ms(t);
        for (i, x) in [ram, cpu, gpu, ssd, 0., ui].into_iter().enumerate() { if i != 4 { self.ms[i] += (x - self.ms[i]) * 0.1 } }
    }
    /// Конец кадра: учёт блиттинга GDI, такты CPU, поиск узкого места, сон до бюджета целевого FPS.
    pub fn end_frame(&mut self, present_ms: f32, sleep: bool) {
        self.present += (present_ms - self.present) * 0.1;
        self.ms[5] += present_ms * 0.1;
        let work = ms(self.last);
        let cyc = tsc().wrapping_sub(self.tsc0) as f32;
        if work > 0.05 && cyc > 0. { self.cpms += (cyc / work - self.cpms) * 0.1 }
        self.frame += (work - self.frame) * 0.1;
        self.ms[4] = self.frame;
        self.glitch = (self.frame / self.budget - 1.).clamp(0., 1.);
        self.bott = if self.frame > self.budget * 1.05 { [0, 1, 2, 3, 5].into_iter().max_by(|&a, &b| self.ms[a].total_cmp(&self.ms[b])) } else { None };
        if sleep && work < self.budget { std::thread::sleep(Duration::from_secs_f32((self.budget - work) / 1000.)) }
    }
    fn stream(&mut self) {
        let (pcx, pcz) = ((self.v.p[0].floor() as i32) >> CB, (self.v.p[2].floor() as i32) >> CB);
        let rad = if self.q.mode == 1 { 7 } else { ((self.q.radius.max(self.q.view as i32) >> CB) + 1).clamp(2, 7) };
        let (mut n, mut pend) = (0, 0);
        for k in 0..self.spiral.len() {
            let (dx, dz) = self.spiral[k];
            let (cx, cz) = (pcx + dx, pcz + dz);
            if dx.abs() > rad || dz.abs() > rad || self.sim.w.slot[si(cx, cz)] == (cx, cz) { continue }
            if n < self.q.chunks {
                let sp = gen_chunk(&mut self.sim.w, &self.wgen, &self.q, cx, cz);
                for (k, p, g) in sp { if self.sim.cre.len() < self.q.max_ent { self.sim.spawn(k, p, g) } }
                n += 1
            } else { pend += 1 }
        }
        (self.pending, self.gen_n) = (pend, n);
    }
    /// Ставит камеру над сушей с самой плотной клеточной жизнью рядом (после генерации нового мира).
    fn place(&mut self) {
        let w = &self.sim.w;
        let (px, pz) = (self.v.p[0].floor() as i32, self.v.p[2].floor() as i32);
        let mut best = (-1, px, pz);
        for cz in (-112..=112).step_by(8) { for cx in (-112..=112).step_by(8) {
            let (x, z) = (px + cx, pz + cz);
            if !w.loaded(x, z) || w.top(x, z) <= self.wgen.sea_v() + 2 { continue }
            let mut s = 0;
            for dz in (-12..=12).step_by(2) { for dx in (-12..=12).step_by(2) { if w.get(x + dx, w.top(x + dx, z + dz), z + dz) == LIFE { s += 1 } } }
            if s > best.0 { best = (s, x, z) }
        } }
        let y = if self.wgen.pl == 3 { H as f32 / 2. } else { (w.top(best.1, best.2) + 32).min(H as i32 + 16) as f32 };
        self.v.p = [best.1 as f32 + 0.5, y, best.2 as f32 + 0.5];
        self.placed = true;
    }
    fn update_parts(&mut self, dt: f32) {
        let (w, g) = (&self.sim.w, 22. * SF * self.sim.pl.g / 9.8);
        for p in &mut self.parts {
            p.v[1] -= g * dt;
            for i in 0..3 { p.p[i] += p.v[i] * dt }
            p.t -= dt;
            if matches!(kind(w.get(p.p[0].floor() as i32, p.p[1].floor() as i32, p.p[2].floor() as i32)), K::Solid | K::Powder) { p.v = [0.; 3]; p.t = p.t.min(0.4) }
        }
        self.parts.retain(|p| p.t > 0.);
    }
    /// Пересоздание мира по запросу (и/или с принудительной планетой).
    fn new_world(&mut self, prompt: &str, pl: Option<u8>) -> Vec<usize> {
        let l = norm(prompt);
        let seed = hash(self.regen as i32, l.len() as i32, l.chars().map(|c| c as i32 % 4096).sum(), 0x5EED);
        self.regen += 1;
        let (mut g, f) = parse_prompt(&l, seed);
        if let Some(p) = pl { g.pl = p; planet_defaults(&mut g) }
        self.sim.pl = PLANETS[g.pl as usize];
        self.wgen = g;
        self.prompt = prompt.into();
        self.sim.w.reset();
        self.sim.bodies.clear();
        self.sim.claimed.clear();
        self.sim.supq.clear();
        self.sim.cre.clear();
        self.parts.clear();
        (self.me, self.placed, self.insp) = (false, false, None);
        f
    }

    // ── ввод ──
    fn input(&mut self, inp: &mut Input, dt: f32) {
        let (mx, my) = self.mouse;
        if inp.hit(0x72) { self.prof = !self.prof } // F3 — профайлер везде
        match self.scr {
            Scr::Menu => {
                if inp.hit(K_UP) { self.menu_sel = (self.menu_sel + 3) % 4 }
                if inp.hit(K_DOWN) { self.menu_sel = (self.menu_sel + 1) % 4 }
                let mut act = inp.hit(K_ENTER);
                if inp.click { if let Some(i) = self.menu_btns().iter().position(|b| inside(mx, my, *b)) { self.menu_sel = i; act = true } }
                if inp.hit(K_ESC) && self.started { self.scr = Scr::Play }
                if act {
                    match self.menu_sel {
                        0 => { self.scr = Scr::Play; if !self.started { self.started = true; self.say("Мир запущен. F1 - справка, Tab - редактор, F3 - отладка CPU".into(), "World started. F1 - help, Tab - editor, F3 - CPU debug".into()) } }
                        1 => self.scr = Scr::Settings,
                        2 => self.ru = !self.ru,
                        _ => self.quit = true,
                    }
                }
                if !self.started { self.v.yaw += dt * 0.07 }
            }
            Scr::Settings => {
                self.sliders(inp);
                if inp.hit(K_UP) { self.sel = (self.sel + 5) % 6 }
                if inp.hit(K_DOWN) { self.sel = (self.sel + 1) % 6 }
                if inp.hit(b'L') { self.ru = !self.ru }
                let [bp, bd, bb] = self.set_btns();
                if inp.lmb {
                    if self.drag.is_none() && inp.click { self.drag = (0..6).find(|&i| { let b = self.sl_bar(i); inside(mx, my, (b.0 - 4, b.1 - 6, b.2 + 8, b.3 + 12)) }) }
                    if let Some(i) = self.drag { let b = self.sl_bar(i); self.sel = i; self.sl[i] = ((mx - b.0) * 100 / b.2).clamp(1, 100) }
                } else { self.drag = None }
                if inp.click && inside(mx, my, bp) {
                    let p = (self.wgen.pl + 1) % 4;
                    let pr = self.prompt.clone();
                    self.new_world(&pr, Some(p));
                    self.say(format!("Планета: {}", PLANETS[p as usize].ru), format!("Planet: {}", PLANETS[p as usize].en));
                }
                if inp.click && inside(mx, my, bd) { self.prof = !self.prof }
                if inp.hit(K_ESC) || inp.hit(K_ENTER) || (inp.click && inside(mx, my, bb)) { self.scr = Scr::Menu }
            }
            Scr::Play => self.play(inp, dt),
        }
    }
    fn sliders(&mut self, inp: &Input) {
        for i in 0..6 { if inp.hit(b'1' + i as u8) { self.sel = i } }
        let st = if inp.held(K_SHIFT) { 10 } else { 1 };
        if inp.hit(K_LEFT) { self.sl[self.sel] = (self.sl[self.sel] - st).max(1) }
        if inp.hit(K_RIGHT) { self.sl[self.sel] = (self.sl[self.sel] + st).min(100) }
    }
    fn play(&mut self, inp: &mut Input, dt: f32) {
        if self.editor {
            for &c in &inp.chars { if self.line.chars().count() < 160 { self.line.push(c) } }
            if inp.hit(K_BACK) { self.line.pop(); }
            if inp.hit(K_ENTER) { let l = std::mem::take(&mut self.line); self.exec(&l) }
            if inp.hit(K_TAB) || inp.hit(K_ESC) { self.editor = false }
            return;
        }
        if inp.hit(K_ESC) { self.scr = Scr::Menu; self.menu_sel = 0; return }
        if inp.hit(K_TAB) { self.editor = true; return }
        if inp.hit(K_F1) { self.help = !self.help }
        if inp.hit(K_BACK) { self.insp = None }
        if inp.hit(b'I') { self.tool = 0 }
        self.sliders(inp);
        if inp.hit(K_UP) { self.brush = (self.brush + 1).min(24) }
        if inp.hit(K_DOWN) { self.brush = (self.brush - 1).max(1) }
        if inp.wheel != 0 { self.tool = (self.tool as i32 - inp.wheel).rem_euclid(TOOLS.len() as i32) as usize }
        if inp.hit(b'E') { self.mat = if self.mat >= BASALT { STONE } else { self.mat + 1 } }
        if inp.hit(b'Q') { self.mat = if self.mat <= STONE { BASALT } else { self.mat - 1 } }
        if inp.hit(b'P') { self.paused = !self.paused }
        if inp.hit(b'F') { self.toggle_me() }
        if self.q.mode == 2 { self.v.yaw += inp.look.0 as f32 * 0.003; self.v.pitch = (self.v.pitch - inp.look.1 as f32 * 0.003).clamp(-1.5, 1.5) }
        self.movement(inp, dt);
        let (mx, my) = self.mouse;
        (self.hover, self.hover_cre) = if my > TOP && my < self.cv.h - 16 { self.pick(mx, my) } else { (None, None) };
        self.cd -= dt;
        if inp.click { if let Some(r) = self.insp_rect() { if inside(mx, my, r) { self.insp = None; return } } }
        if (inp.lmb || inp.click) && !inp.rmb && (self.cd <= 0. || inp.click) && my > TOP && my < self.cv.h - 16 { self.apply_tool(inp.click) }
    }
    fn solid_at(&self, f: V3) -> bool {
        let (hw, hh) = (0.3 * SF, 1.75 * SF);
        for y in f[1].floor() as i32..=(f[1] + hh).floor() as i32 { for z in (f[2] - hw).floor() as i32..=(f[2] + hw).floor() as i32 { for x in (f[0] - hw).floor() as i32..=(f[0] + hw).floor() as i32 {
            if matches!(kind(self.sim.w.get(x, y, z)), K::Solid | K::Powder) { return true }
        } } }
        false
    }
    /// Бог летает свободно (и может подняться в космос); человек ходит с гравитацией планеты, прыгает, плавает,
    /// в поясе астероидов летает на ранце.
    fn movement(&mut self, inp: &Input, dt: f32) {
        let k = |c: u8| inp.held(c) as i32 as f32;
        let (fw, st, up) = (k(b'W') - k(b'S'), k(b'D') - k(b'A'), k(K_SPACE) - k(b'C'));
        let (mode, cy, sy) = (self.q.mode, self.v.yaw.cos(), self.v.yaw.sin());
        if st != 0. { self.face = st }
        if !self.me {
            let sp = if inp.held(K_SHIFT) { 120. } else { 36. } * dt * if self.v.p[1] > H as f32 * 2. { 4. } else { 1. };
            let d = match mode { 2 => { let (f, r, _) = basis(self.v.yaw, self.v.pitch); add(add(sc(f, fw), sc(r, st)), [0., up, 0.]) } 1 => [st, fw, up * 0.25], _ => [st, up, -fw] };
            for i in 0..3 { self.v.p[i] += d[i] * sp }
            self.v.p[1] = self.v.p[1].clamp(1., H as f32 + 1200.);
            return;
        }
        let pl = self.sim.pl;
        let g = pl.g * 2.55 * SF;
        let wish = match mode { 2 => [cy * fw - sy * st, 0., sy * fw + cy * st], 1 => [st, 0., 0.], _ => [st, 0., -fw] };
        let spd = if inp.held(K_SHIFT) { 9. } else { 5. } * SF;
        let mut feet = [self.v.p[0], self.v.p[1] - 1.6 * SF, self.v.p[2]];
        let wet = kind(self.sim.w.get(feet[0].floor() as i32, (feet[1] + 1.).floor() as i32, feet[2].floor() as i32)) == K::Liquid;
        (self.vel[0], self.vel[2]) = (wish[0] * spd, wish[2] * spd);
        self.vel[1] -= g * dt;
        if pl.float { self.vel[1] += up * 30. * dt; self.vel[1] *= 0.995 } // ранец в невесомости
        if wet { self.vel[1] = self.vel[1].max(-6.); if inp.held(K_SPACE) { self.vel[1] = 8. } }
        if inp.held(K_SPACE) && self.ground && !pl.float { self.vel[1] = 8.5 * SF }
        let was = self.ground;
        self.ground = false;
        for a in [0, 2, 1] {
            let mut n = feet;
            n[a] += self.vel[a] * dt;
            if !self.solid_at(n) { feet = n } else if a != 1 { n[1] += 1.05; if was && !self.solid_at(n) { feet = n } } else { if self.vel[1] < 0. { self.ground = true } self.vel[1] = 0. }
        }
        if mode == 1 { feet[2] = feet[2].floor() + 0.5 }
        if feet[1] < -10. { feet[1] = self.sim.w.top(feet[0] as i32, feet[2] as i32) as f32 + 2.; self.vel[1] = 0. }
        self.v.p = [feet[0], feet[1] + 1.6 * SF, feet[2]];
    }
    fn toggle_me(&mut self) {
        self.me = !self.me;
        self.vel = [0.; 3];
        if self.me {
            let (x, z) = match self.hover { Some(h) if self.q.mode != 0 => (h.prev[0], h.prev[2]), _ => (self.v.p[0].floor() as i32, self.v.p[2].floor() as i32) };
            let y = if self.sim.pl.float { self.v.p[1] } else { self.sim.w.top(x, z) as f32 + 1. + 1.6 * SF + 0.01 };
            self.v.p = [x as f32 + 0.5, y, z as f32 + 0.5];
            self.say("Вы воплотились в человека: WASD - идти, Space - прыжок, F - выйти".into(), "You became a human: WASD walk, Space jump, F to leave".into());
        } else { self.say("Вы снова бог-наблюдатель".into(), "You are the watching god again".into()) }
    }
    /// Что под курсором: воксель (3D — луч из пикселя, 2D — клетка среза, текст — столбец) и существо, если оно ближе.
    fn pick(&self, mx: i32, my: i32) -> (Option<Hit>, Option<u32>) {
        let w = &self.sim.w;
        let mut rs = RStats::default();
        match self.q.mode {
            2 => {
                let (f, r, u) = basis(self.v.yaw, self.v.pitch);
                let asp = self.cv.w as f32 / self.cv.h as f32;
                let (sx, sy) = ((2. * mx as f32 / self.cv.w as f32 - 1.) * FOV * asp, (1. - 2. * my as f32 / self.cv.h as f32) * FOV);
                let d = nrm(add(f, add(sc(r, sx), sc(u, sy))));
                let h = dda(w, self.v.p, d, 320., AIR, &mut rs).filter(|h| h.m != VOID && h.p[1] >= 0);
                let ht = h.map_or(f32::INFINITY, |h| h.t);
                let c = self.sim.cre.iter().filter(|c| self.cre_active(c)).filter_map(|c| ray_model(model(c.k), c.p, c.yaw, self.v.p, d).map(|x| (x.0, c.id))).filter(|x| x.0 < ht).min_by(|a, b| a.0.total_cmp(&b.0));
                (h, c.map(|c| c.1))
            }
            1 => {
                let p = [(self.v.p[0] + (mx - self.cv.w / 2) as f32 / 3.).floor() as i32, (self.v.p[1] + (self.cv.h / 2 - my) as f32 / 3.).floor() as i32, self.v.p[2].floor() as i32];
                let c = self.sim.cre.iter().filter(|c| self.cre_active(c) && (c.p[2] - self.v.p[2]).abs() < 8.)
                    .find(|c| { let m = model(c.k); (p[0] as f32 + 0.5 - c.p[0]).abs() < m.sx as f32 * MC / 2. && p[1] as f32 + 0.5 >= c.p[1] && p[1] as f32 + 0.5 <= c.p[1] + m.sy as f32 * MC }).map(|c| c.id);
                (Some(Hit { p, prev: p, m: w.get(p[0], p[1], p[2]), t: 0., ax: 1, s: -1 }), c)
            }
            _ => {
                let (s, vpc) = (self.q.txt, self.q.vpc);
                let (cols, rows) = (self.cv.w / (6 * s), (self.cv.h - TOP - 16) / (11 * s));
                let (cc, rr) = (mx / (6 * s) - cols / 2, (my - TOP) / (11 * s) - rows / 2);
                let (x, z) = (self.v.p[0].floor() as i32 + cc * vpc, self.v.p[2].floor() as i32 + rr * vpc);
                let t = w.top(x, z);
                let c = self.sim.cre.iter().filter(|c| self.cre_active(c))
                    .find(|c| (c.p[0].floor() as i32 - self.v.p[0].floor() as i32).div_euclid(vpc) == cc && (c.p[2].floor() as i32 - self.v.p[2].floor() as i32).div_euclid(vpc) == rr).map(|c| c.id);
                (w.loaded(x, z).then_some(Hit { p: [x, t, z], prev: [x, t + 1, z], m: w.get(x, t, z), t: 0., ax: 1, s: -1 }), c)
            }
        }
    }
    fn apply_tool(&mut self, click: bool) {
        let (b, lvl) = (self.brush, self.q.level);
        self.cd = 0.05;
        if self.tool == 0 { // осмотр
            if click { let (mx, my) = self.mouse; self.insp = match (self.hover_cre, self.hover) { (Some(id), _) => Some((Insp::Cre(id), mx, my)), (_, Some(h)) => Some((Insp::Vox(h.p), mx, my)), _ => None } }
            return;
        }
        let Some(h) = self.hover else { return };
        match self.tool {
            1 => if click { self.sim.explode(h.p, b + 2, lvl); self.say(format!("Взрыв радиусом {:.1} м", (b + 2) as f32 / SF), format!("Blast, radius {:.1} m", (b + 2) as f32 / SF)) },
            2 => { let m = self.mat; self.sphere(h.prev, b - 1, m, 1., |o| dens(o) < 20) }
            3 => self.sphere(h.p, b, AIR, 1., |o| o != BEDROCK && o != VOID),
            4 => self.sphere(h.prev, b, FIRE, 0.35, |o| o == AIR),
            5 => {
                let r = b * 2;
                for dz in -r..=r { for dx in -r..=r {
                    if dx * dx + dz * dz > r * r || !self.sim.chance(0.3) { continue }
                    let (x, z) = (h.p[0] + dx, h.p[2] + dz);
                    let t = self.sim.w.top(x, z);
                    if matches!(kind(self.sim.w.get(x, t, z)), K::Solid | K::Powder) { self.sim.put(x, t + 1, z, LIFE) }
                } }
            }
            k => if click {
                let k = (k - 6) as u8;
                let p = [h.prev[0] as f32 + 0.5, 0., h.prev[2] as f32 + 0.5];
                let grp = if k == 0 { self.sim.cre.iter().filter(|c| c.k == 0 && dist_h(c.p, p) < 60.).map(|c| c.group).next().unwrap_or(0) } else { 0 };
                self.sim.spawn(k, p, grp);
                let need = if k == 2 { 4 } else { 3 };
                if lvl < need { self.say(format!("Существо создано, но оживёт с уровня {} (поднимите ЦП)", need), format!("Creature created; it comes alive at level {} (raise CPU)", need)) }
            },
        }
    }
    fn sphere(&mut self, c: [i32; 3], r: i32, m: u8, p: f32, ok: impl Fn(u8) -> bool) {
        for dz in -r..=r { for dy in -r..=r { for dx in -r..=r {
            let (x, y, z) = (c[0] + dx, c[1] + dy, c[2] + dz);
            if dx * dx + dy * dy + dz * dz <= r * r && ok(self.sim.w.get(x, y, z)) && self.sim.chance(p) { self.sim.put(x, y, z, m) }
        } } }
    }
    /// Команда редактора: правило, служебная команда или описание мира на естественном языке.
    fn exec(&mut self, line: &str) {
        let ws = words(line);
        let Some(first) = ws.first().cloned() else { return };
        let ru = self.ru;
        self.out.push(format!("> {}", line.trim()));
        match first.as_str() {
            "если" | "if" => match parse_rule(line) {
                Some(r) => {
                    let t = rule_txt(&r, ru);
                    self.sim.rules.push(r);
                    self.sim.reindex();
                    self.out.push(if ru { format!("Правило #{} добавлено: {}", self.sim.rules.len(), t) } else { format!("Rule #{} added: {}", self.sim.rules.len(), t) });
                    if self.q.level < 2 { self.out.push(self.l("Правила работают с уровня 2: поднимите ползунок ЦП выше 30%", "Rules run from level 2: raise the CPU slider above 30%").into()) }
                }
                None => self.out.push(self.l("Не понял правило. Формат: если <A> [рядом <B>] то <C> <N>%", "Can't parse. Format: if <A> [near <B>] then <C> <N>%").into()),
            },
            "правила" | "rules" => { for (i, r) in self.sim.rules.iter().enumerate() { self.out.push(format!("#{} {}", i + 1, rule_txt(r, ru))) } }
            "удалить" | "del" | "delete" => match ws.get(1).and_then(|s| s.parse::<usize>().ok()).filter(|&n| n >= 1 && n <= self.sim.rules.len()) {
                Some(n) => { self.sim.rules.remove(n - 1); self.sim.reindex(); self.out.push(self.l("Удалено.", "Removed.").into()) }
                None => self.out.push(self.l("Нет такого номера (см. \"правила\")", "No such number (see 'rules')").into()),
            },
            "сброс" | "reset" => { self.sim.default_rules(); self.out.push(self.l("Правила по умолчанию восстановлены.", "Default rules restored.").into()) }
            "очистить" | "clear" => { self.sim.rules.clear(); self.sim.reindex(); self.out.push(self.l("Все правила удалены - химия выключена.", "All rules removed - chemistry is off.").into()) }
            "помощь" | "help" => {
                let lines = if ru { ["Мир: опишите словами - остров, горы, лес, пустыня, лёд, вулкан, пещеры, город, руины, жизнь, животные, люди",
                    "Планеты: луна, марс, пояс астероидов (космос), земля. Правило: если <A> [рядом <B>] то <C> <N>%",
                    "Команды: правила | удалить N | сброс | очистить | помощь"] }
                else { ["World: describe it - island, mountains, forest, desert, ice, volcano, caves, city, ruins, life, animals, people",
                    "Planets: moon, mars, asteroid belt (space), earth. Rule: if <A> [near <B>] then <C> <N>%",
                    "Commands: rules | del N | reset | clear | help"] };
                self.out.extend(lines.iter().map(|s| s.to_string()));
                let names: Vec<&str> = MATS.iter().skip(1).map(|m| if ru { m.ru } else { m.en }).collect();
                for c in names.chunks(10) { self.out.push(format!("{}: {}", self.l("Вещества", "Materials"), c.join(", "))) }
            }
            _ => {
                let f = self.new_world(line, None);
                let names: Vec<&str> = f.iter().map(|&i| if ru { FEATS[i].1 } else { FEATS[i].2 }).collect();
                self.out.push(if f.is_empty() { self.l("Понятий не найдено - создаю случайный мир.", "No known concepts - generating a random world.").to_string() }
                    else if ru { format!("Создаю мир: {}", names.join(", ")) } else { format!("Generating: {}", names.join(", ")) });
                let pl = PLANETS[self.wgen.pl as usize];
                if self.wgen.pl != 0 { self.out.push(if ru { format!("Планета {}: гравитация {:.2} м/с2, атмосфера {:.0}%", pl.ru, pl.g, pl.atm * 100.) } else { format!("Planet {}: gravity {:.2} m/s2, atmosphere {:.0}%", pl.en, pl.g, pl.atm * 100.) }) }
                if f.contains(&1) || f.contains(&12) { self.out.push(self.l("Животные оживают с уровня 3 (ЦП > 50%), люди - с уровня 4 (ЦП > 75%).", "Animals come alive at level 3 (CPU > 50%), people at level 4 (CPU > 75%).").into()) }
                if f.contains(&8) { self.out.push(self.l("Клеточная жизнь (Конвей) идёт на любом уровне: зелёные клетки на поверхности.", "Cellular life (Conway) runs at any level: green cells on the surface.").into()) }
            }
        }
        let n = self.out.len();
        if n > 300 { self.out.drain(..n - 300); }
    }

    // ── вывод ──
    fn render_world(&mut self) {
        let (q, e) = (self.q, self.env());
        if q.mode == 0 { self.cv.px.fill(0x05080a); self.thr.clear(); render_text(&mut self.cv, &self.sim.w, &self.v, &q, self.me, &self.sim.cre, q.level); return }
        let (ww, wh) = (((self.cv.w as f32 * q.res) as usize).max(32), ((self.cv.h as f32 * q.res) as usize).max(18));
        if (ww, wh) != (self.ww, self.wh) { (self.ww, self.wh, self.wb, self.zb) = (ww, wh, vec![0; ww * wh], vec![0.; ww * wh]) }
        let ppv = 3. * ww as f32 / self.cv.w as f32;
        let (f, r, u) = basis(self.v.yaw, self.v.pitch);
        let asp = ww as f32 / wh as f32;
        if q.mode == 2 {
            let mut vis = vec![];
            for c in self.sim.cre.iter().filter(|c| self.cre_active(c)) { // отбор видимых существ и их экранные прямоугольники
                let m = model(c.k);
                let ctr = add(c.p, [0., m.sy as f32 * MC / 2., 0.]);
                let rel = sub(ctr, self.v.p);
                let z = dot(rel, f);
                if z < 0.5 || z > q.view { continue }
                let (px, py) = ((dot(rel, r) / (z * FOV * asp) + 1.) * 0.5 * ww as f32, (1. - dot(rel, u) / (z * FOV)) * 0.5 * wh as f32);
                let rad = (m.sx.max(m.sy).max(m.sz) as f32 * MC * 0.75) / (z * FOV) * wh as f32 / 2. + 1.;
                let rect = ((px - rad) as i32, (py - rad) as i32, (px + rad) as i32, (py + rad) as i32);
                if rect.2 < 0 || rect.3 < 0 || rect.0 >= ww as i32 || rect.1 >= wh as i32 { continue }
                vis.push(VisCre { p: c.p, yaw: c.yaw, k: c.k, col: c.col, tint: 0.85 + (c.id % 7) as f32 * 0.05, r: rect });
            }
            self.thr = render3d(&self.sim.w, &self.v, &q, &e, &vis, &mut self.wb, &mut self.zb, ww, wh);
        } else {
            self.thr = render2d(&self.sim.w, &self.v, &q, &e, &mut self.wb, &mut self.zb, ww, wh, ppv);
            let lv = q.level;
            for c in &self.sim.cre { if c.alive && lv >= 3 && (c.k < 2 || lv >= 4) && (c.p[2] - self.v.p[2]).abs() < 8. { draw_cre2d(&mut self.wb, ww, wh, ppv, &self.v, c.p, c.yaw, c.k, c.col) } }
            if self.me { let p = [self.v.p[0], self.v.p[1] - 1.6 * SF, self.v.p[2]]; draw_cre2d(&mut self.wb, ww, wh, ppv, &self.v, p, if self.face > 0. { 0. } else { 3.14 }, 2, 0xe8e8e8) }
        }
        for p in &self.parts { // частицы обломков с проверкой глубины
            let rel = sub(p.p, self.v.p);
            let (px, py, s, d) = if q.mode == 2 {
                let z = dot(rel, f);
                if z < 0.3 { continue }
                (((dot(rel, r) / (z * FOV * asp) + 1.) * 0.5 * ww as f32) as i32, ((1. - dot(rel, u) / (z * FOV)) * 0.5 * wh as f32) as i32, ((0.05 * wh as f32 / (z * FOV)) as i32).clamp(1, 8), dot(rel, rel).sqrt())
            } else {
                if rel[2].abs() > 6. { continue }
                ((ww as f32 / 2. + rel[0] * ppv) as i32, (wh as f32 / 2. - rel[1] * ppv) as i32, (ppv * 0.6).max(1.) as i32, 0.)
            };
            for y in py.max(0)..(py + s).min(wh as i32) { for x in px.max(0)..(px + s).min(ww as i32) {
                let i = y as usize * ww + x as usize;
                if q.mode != 2 || d < self.zb[i] { self.wb[i] = p.c }
            } }
        }
    }
    fn compose(&mut self) {
        if self.q.mode != 0 { upscale(&mut self.cv, &self.wb, self.ww, self.wh) }
        if self.glitch > 0.02 { self.glitches() }
        match self.scr {
            Scr::Menu => self.draw_menu(),
            Scr::Settings => self.draw_settings(),
            Scr::Play => { self.draw_hud(); self.draw_insp(); if self.editor { self.draw_editor() } if self.help { self.draw_help() } }
        }
        if self.prof { self.draw_prof() }
        self.draw_telemetry();
    }
    /// Троттлинг: сдвиги строк, расслоение каналов и «битые» блоки — тем сильнее, чем больше перерасход бюджета.
    fn glitches(&mut self) {
        let (g, w, h) = (self.glitch, self.cv.w, self.cv.h);
        for _ in 0..(g * 40.) as i32 + 1 {
            let (y0, hh) = (TOP + (self.rf() * (h - TOP) as f32) as i32, 1 + (self.rf() * 6.) as i32);
            let (sh, swap) = (((self.rf() - 0.5) * g * 80.) as i32, self.rf() < 0.3);
            for y in y0..(y0 + hh).min(h) {
                let row = &mut self.cv.px[(y * w) as usize..((y + 1) * w) as usize];
                row.rotate_right(sh.rem_euclid(w) as usize);
                if swap { for p in row.iter_mut() { *p = (*p & 0xff00) | (*p >> 16 & 255) | ((*p & 255) << 16) } }
            }
        }
        for _ in 0..(g * 8.) as i32 {
            let (x, y, s, c) = ((self.rf() * w as f32) as i32, TOP + (self.rf() * (h - TOP) as f32) as i32, 4 + (self.rf() * 24.) as i32, (self.rf() * 16_777_215.) as u32);
            self.cv.rect(x, y, s * 2, s, c, 160);
        }
    }
    fn draw_telemetry(&mut self) {
        let (q, ru, cw, lg) = (self.q, self.ru, self.cv.w, !self.ru as usize);
        self.cv.rect(0, 0, cw, TOP - 2, PANEL, 215);
        let (lv, md, mem, pl) = (LEVELS[q.level as usize][lg], MODES[q.mode as usize][lg], self.sim.w.mem_bytes as f32 / 1_048_576., PLANETS[self.wgen.pl as usize]);
        let n = self.sim.cre.iter().filter(|c| self.cre_active(c)).count();
        let s = if ru { format!("ЦЕЛЬ {:.0} FPS  РЕАЛ {:.1}  КАДР {:.1}/{:.1} мс  РАДИУС {} м  УР.{} {}  ВИД {}  ТИК {:.0} Гц  ПАМЯТЬ {:.1}/{:.0} МБ  ЧАНКИ +{}  СУЩЕСТВ {}  {}", q.fps, self.fps, self.frame, self.budget, q.radius / S, q.level, lv, md, q.tick_hz, mem, q.mem_mb, self.pending, n, pl.ru) }
            else { format!("TARGET {:.0} FPS  REAL {:.1}  FRAME {:.1}/{:.1} ms  RADIUS {} m  LVL {} {}  VIEW {}  TICK {:.0} Hz  MEMORY {:.1}/{:.0} MB  CHUNKS +{}  CREATURES {}  {}", q.fps, self.fps, self.frame, self.budget, q.radius / S, q.level, lv, md, q.tick_hz, mem, q.mem_mb, self.pending, n, pl.en) };
        self.cv.text(4, 3, &s, if self.bott.is_some() { RED } else { TXT }, 1);
        let bw = (cw - 8) / 6;
        let blink = (self.time * 5.) as i32 % 2 == 0;
        for i in 0..6 {
            let (x, y, bn) = (4 + i as i32 * bw, 16, self.bott == Some(i));
            let load = (if i == 4 { self.frame } else { self.ms[i] }) / self.budget * 100.;
            self.cv.rect(x, y, bw - 4, 34, if bn && blink { 0x6a1810 } else { 0x1a2026 }, 230);
            if i == self.sel { self.cv.frame(x, y, bw - 4, 34, ACC) } else if bn { self.cv.frame(x, y, bw - 4, 34, RED) }
            let extra = if i == 5 { format!(" {}", md) } else { String::new() };
            self.cv.text(x + 4, y + 4, &format!("{} {} {}%{}", i + 1, SL_NAME[i][lg], self.sl[i], extra), if i == self.sel { ACC } else { TXT }, 1);
            let lt = format!("{:.0}%", load);
            self.cv.text(x + bw - 8 - tw(&lt, 1), y + 4, &lt, if load > 100. { RED } else { DIM }, 1);
            let bc = if load > 100. { RED } else if load > 70. { ACC } else { 0x5ad06a };
            self.cv.rect(x + 4, y + 20, bw - 12, 8, 0x30383f, 255);
            self.cv.rect(x + 4, y + 20, ((bw - 12) as f32 * (load / 100.).min(1.)) as i32, 8, bc, 255);
        }
        if let Some(b) = self.bott {
            let s = if ru { format!("! УЗКОЕ МЕСТО: {} ({:.1} мс при бюджете кадра {:.1} мс) - снизьте ползунок {} или целевой FPS", SL_NAME[b][0], self.ms[b], self.budget, b + 1) }
                else { format!("! BOTTLENECK: {} ({:.1} ms of a {:.1} ms frame budget) - lower slider {} or the target FPS", SL_NAME[b][1], self.ms[b], self.budget, b + 1) };
            self.cv.rect(0, TOP - 2, cw, 14, 0x400c08, 220);
            self.cv.text(4, TOP, &s, if blink { RED } else { 0xffb0a0 }, 1);
        }
    }
    fn draw_hud(&mut self) {
        let (cw, ch, ru, q, lg) = (self.cv.w, self.cv.h, self.ru, self.q, !self.ru as usize);
        if self.lag > 0. { let s = self.l("Симуляция не успевает: тики пропущены", "Simulation can't keep up: ticks skipped"); self.cv.text(4, TOP + 16, s, RED, 1); }
        let mut y = ch - 30;
        for (r, e, t) in self.log.iter().rev().take(6) {
            let age = self.time - t;
            if age > 12. { break }
            self.cv.rect(4, y - 1, tw(if ru { r } else { e }, 1) + 4, 12, PANEL, 150);
            self.cv.text(6, y, if ru { r } else { e }, mix(TXT, 0x505860, age / 12.), 1);
            y -= 13;
        }
        let mn = if ru { MATS[self.mat as usize].ru } else { MATS[self.mat as usize].en };
        let s = if ru { format!("[Колесо] {}  [Q/E] {}  [Вверх/Вниз] кисть {}  [I] осмотр  [F] {}  [Tab] редактор  [F1] помощь  [F3] отладка  [P] {}  [Esc] меню", TOOLS[self.tool][0], mn, self.brush, if self.me { "выйти из тела" } else { "вселиться" }, if self.paused { "продолжить" } else { "пауза" }) }
            else { format!("[Wheel] {}  [Q/E] {}  [Up/Down] brush {}  [I] inspect  [F] {}  [Tab] editor  [F1] help  [F3] debug  [P] {}  [Esc] menu", TOOLS[self.tool][1], mn, self.brush, if self.me { "leave body" } else { "incarnate" }, if self.paused { "resume" } else { "pause" }) };
        self.cv.rect(0, ch - 16, cw, 16, PANEL, 215);
        self.cv.text(4, ch - 13, &s, TXT, 1);
        let label = match (self.hover_cre.and_then(|id| self.sim.cre.iter().find(|c| c.id == id)), self.hover) {
            (Some(c), _) => Some(format!("{} {} #{} - {}", TOOLS[self.tool][lg], CRE_NAME[c.k as usize][lg], c.id, ST_NAME[c.st as usize][lg])),
            (_, Some(h)) => {
                if q.mode == 1 { let (x, y) = (cw / 2 + ((h.p[0] as f32 - self.v.p[0]) * 3.).floor() as i32, ch / 2 - ((h.p[1] as f32 + 1. - self.v.p[1]) * 3.).floor() as i32); self.cv.frame(x, y, 3, 3, 0xffffff) }
                let n = if h.m == VOID { "?" } else if ru { MATS[h.m as usize].ru } else { MATS[h.m as usize].en };
                Some(format!("{} {} [{} {} {}]", TOOLS[self.tool][lg], n, h.p[0], h.p[1], h.p[2]))
            }
            _ => None,
        };
        if let Some(s) = label {
            let (mx, my) = self.mouse;
            self.cv.rect(mx + 12, my + 10, tw(&s, 1) + 6, 13, PANEL, 200);
            self.cv.text(mx + 15, my + 12, &s, TXT, 1);
        }
        let (x, y) = (cw / 2, ch / 2);
        if self.me && q.mode == 2 { self.cv.rect(x - 6, y, 13, 1, 0xffffff, 220); self.cv.rect(x, y - 6, 1, 13, 0xffffff, 220) }
        if self.paused { let s = self.l("ПАУЗА", "PAUSED"); self.cv.text((cw - tw(s, 3)) / 2, TOP + 44, s, ACC, 3); }
    }
    /// Строки окна инспектора: название, поведение/состояние, параметры, координаты, стоимость на ЦП.
    fn insp_lines(&self) -> Option<Vec<String>> {
        let (ins, _, _) = self.insp?;
        let (ru, lg, tick_us) = (self.ru, !self.ru as usize, 1e6 / self.q.tick_hz);
        let st = &self.sim.stt;
        let coord = |p: V3| if ru { format!("Позиция: {:.1} {:.1} {:.1} вокс. = {:.1} {:.1} {:.1} м", p[0], p[1], p[2], p[0] / SF, p[1] / SF, p[2] / SF) } else { format!("Position: {:.1} {:.1} {:.1} vox = {:.1} {:.1} {:.1} m", p[0], p[1], p[2], p[0] / SF, p[1] / SF, p[2] / SF) };
        match ins {
            Insp::Cre(id) => {
                let c = self.sim.cre.iter().find(|c| c.id == id)?;
                let k = c.k as usize;
                let tot = st.ai_ns[k] / 1000.;
                let tgt = if matches!(c.st, S_FLEE | S_HUNT) { format!(" #{}", c.other) } else { String::new() };
                Some(vec![
                    format!("{} #{}{}", CRE_NAME[k][lg], c.id, if k == 0 { format!(" ({} {})", self.l("стадо", "herd"), c.group % 1000) } else { String::new() }),
                    if ru { format!("Поведение: {}{}", ST_NAME[c.st as usize][0], tgt) } else { format!("Behaviour: {}{}", ST_NAME[c.st as usize][1], tgt) },
                    if ru { format!("Еда {:.0}%  Вода {:.0}%  Здоровье {:.0}%  Силы {:.0}%  Возраст {:.0} с{}", c.food * 100., c.water * 100., c.hp * 100., c.stam * 100., c.age, if k == 2 { format!("  Брёвна {}  Построено {}", c.inv, c.built) } else { String::new() }) }
                    else { format!("Food {:.0}%  Water {:.0}%  Health {:.0}%  Stamina {:.0}%  Age {:.0} s{}", c.food * 100., c.water * 100., c.hp * 100., c.stam * 100., c.age, if k == 2 { format!("  Logs {}  Built {}", c.inv, c.built) } else { String::new() }) },
                    coord(c.p),
                    if ru { format!("ЦП: {:.2} мкс за тик ({:.3}% тика); все {}: {:.0} мкс ({:.2}%)", c.ns / 1000., c.ns / 1000. / tick_us * 100., ["травоядные", "хищники", "люди"][k], tot, tot / tick_us * 100.) }
                    else { format!("CPU: {:.2} us per tick ({:.3}% of tick); all {}: {:.0} us ({:.2}%)", c.ns / 1000., c.ns / 1000. / tick_us * 100., ["herbivores", "predators", "humans"][k], tot, tot / tick_us * 100.) },
                ])
            }
            Insp::Vox(p) => {
                let w = &self.sim.w;
                let m = w.get(p[0], p[1], p[2]);
                if m == VOID { return None }
                let mm = &MATS[m as usize];
                let near = (p[0] - self.v.p[0] as i32).abs().max((p[2] - self.v.p[2] as i32).abs()) <= self.q.radius;
                let awake = w.dirty[sec(p[0], p[1], p[2])] || w.ndirty[sec(p[0], p[1], p[2])];
                let state = match (self.q.level >= 1 && near, awake, m == LIFE) {
                    (_, _, true) => { let n = (-1..=1).flat_map(|dz| (-1..=1).map(move |dx| (dx, dz))).filter(|&(dx, dz)| (dx, dz) != (0, 0) && w.get(p[0] + dx, w.top(p[0] + dx, p[2] + dz), p[2] + dz) == LIFE).count(); format!("{} {}", self.l("живая клетка Конвея, соседей:", "Conway cell, neighbours:"), n) }
                    (false, _, _) => self.l("вне радиуса симуляции - заморожено", "outside simulation radius - frozen").into(),
                    (true, true, _) => self.l("секция 16^3 активна - симулируется каждый тик", "16^3 section awake - simulated every tick").into(),
                    (true, false, _) => self.l("секция 16^3 спит - 0 тактов, пока её не тронут", "16^3 section asleep - 0 cycles until touched").into(),
                };
                let rules = self.sim.rules.iter().filter(|r| r.a == m || r.b == m).count();
                let homog = if w.mac[mi(p[0], p[1], p[2])] == MIXED { self.l("смешанный", "mixed") } else { self.l("однородный (луч перепрыгивает)", "uniform (rays skip it)") };
                let (ns, vis) = (st.mat_ns[m as usize], st.visits[m as usize]);
                let tot = ns * vis as f32 / 1000.;
                Some(vec![
                    format!("{} - {}", if ru { mm.ru } else { mm.en }, KIND_NAME[mm.k as usize][lg]),
                    format!("{}: {}", self.l("Состояние", "State"), state),
                    if ru { format!("Плотность {}, правил с участием: {}, блок 4^3: {}", mm.dens, rules, homog) } else { format!("Density {}, rules involved: {}, 4^3 block: {}", mm.dens, rules, homog) },
                    coord([p[0] as f32, p[1] as f32, p[2] as f32]),
                    if ru { format!("ЦП: ~{:.0} нс за обход вокселя; всё вещество: {} обходов = {:.0} мкс ({:.2}% тика)", ns, vis, tot, tot / tick_us * 100.) }
                    else { format!("CPU: ~{:.0} ns per voxel visit; whole material: {} visits = {:.0} us ({:.2}% of tick)", ns, vis, tot, tot / tick_us * 100.) },
                ])
            }
        }
    }
    fn insp_rect(&self) -> Option<(i32, i32, i32, i32)> {
        let lines = self.insp_lines()?;
        let (_, ax, ay) = self.insp?;
        let w = lines.iter().map(|s| tw(s, 1)).max().unwrap_or(0) + 16;
        let h = lines.len() as i32 * 13 + 10;
        Some(((ax + 14).min(self.cv.w - w - 4).max(4), (ay + 14).min(self.cv.h - h - 20).max(TOP + 4), w, h))
    }
    fn draw_insp(&mut self) {
        let (Some(lines), Some(r)) = (self.insp_lines(), self.insp_rect()) else { if self.insp.is_some() && self.insp_lines().is_none() { self.insp = None } return };
        self.cv.rect(r.0, r.1, r.2, r.3, PANEL, 238);
        self.cv.frame(r.0, r.1, r.2, r.3, ACC);
        self.cv.text(r.0 + r.2 - 12, r.1 + 3, "x", DIM, 1);
        for (i, s) in lines.iter().enumerate() { self.cv.text(r.0 + 8, r.1 + 5 + i as i32 * 13, s, if i == 0 { ACC } else if i == 4 { 0x9ad0ff } else { TXT }, 1); }
    }
    /// Профайлер CPU: такты/мс на потоки рендера, пропуск пустоты, физику, ИИ, генерацию, сборку и GDI.
    fn draw_prof(&mut self) {
        let (ru, lg) = (self.ru, !self.ru as usize);
        let cy = |ms: f32, cpms: f32| if cpms > 0. { format!("{:.2} мс / {:.1} Мт", ms, ms * cpms / 1e6) } else { format!("{:.2} ms", ms) };
        let cpms = self.cpms;
        let st = self.sim.stt.clone();
        let mut l: Vec<(String, u32)> = vec![];
        let t = |r: &str, e: &str| if ru { r.to_string() } else { e.to_string() };
        l.push((format!("{}   TSC ~{:.2} {}", t("ОТЛАДКА CPU (F3)", "CPU DEBUG (F3)"), cpms / 1e6, t("ГГц", "GHz")), ACC));
        l.push((format!("{} {}  ({} {:.1})", t("Кадр:", "Frame:"), cy(self.frame, cpms), t("бюджет", "budget"), self.budget), TXT));
        let rt: f32 = self.thr.iter().map(|x| x.0).fold(0., f32::max);
        let (steps, skips, big): (u64, u64, u64) = self.thr.iter().fold((0, 0, 0), |a, x| (a.0 + x.1.steps, a.1 + x.1.skips, a.2 + x.1.big));
        l.push((format!("{} ({} {}, {} {}): {}", t("РЕНДЕР", "RENDER"), self.thr.len(), t("потоков", "threads"), self.thr.len() * 4, t("полос", "bands"), cy(self.ms[2], cpms)), 0x9ad0ff));
        for (i, (m, _, px)) in self.thr.iter().enumerate().take(16) { l.push((format!("  {} {:>2}: {:>6.2} {}  {:>7} {}  {}", t("поток", "thread"), i + 1, m, t("мс", "ms"), px, t("пикс.", "px"), "|".repeat(((m / rt.max(0.01)) * 20.) as usize)), DIM)) }
        if self.thr.len() > 16 { l.push((format!("  ... +{}", self.thr.len() - 16), DIM)) }
        l.push((format!("  {} {} x 16^3 + {} x 4^3 {}, {} {}", t("Пропуск пустоты:", "Empty skipping:"), big, skips, t("прыжков", "jumps"), steps, t("шагов по вокселям", "voxel steps")), TXT));
        l.push((format!("  {} ~{:.0}% {}", t("лучи прошли", "rays covered"), (big * 16 + skips * 4) as f32 * 100. / (big * 16 + skips * 4 + steps).max(1) as f32, t("пути прыжками", "of the path by jumps")), TXT));
        l.push((format!("  {} {:.0}%, {} {} / {:.2} {}", t("однородных блоков", "uniform blocks"), self.homog * 100., t("перепроверено", "rechecked"), self.mac_n, self.mac_ms, t("мс", "ms")), TXT));
        l.push((format!("{} {}", t("ФИЗИКА ВОКСЕЛЕЙ:", "VOXEL PHYSICS:"), cy(st.phys_ms, cpms)), 0x9ad0ff));
        l.push((format!("  {} {}, {} {}", t("секций 16^3", "16^3 sections"), st.secs, t("обходов", "visits"), st.cells), TXT));
        let mut mats: Vec<(usize, f32)> = (0..MATS.len()).map(|i| (i, st.mat_ns[i] * st.visits[i] as f32 / 1000.)).filter(|x| x.1 > 0.).collect();
        mats.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (i, us) in mats.iter().take(3) { l.push((format!("  {}: {:.0} {} x {} = {:.0} {}", if ru { MATS[*i].ru } else { MATS[*i].en }, st.mat_ns[*i], t("нс", "ns"), st.visits[*i], us, t("мкс", "us")), DIM)) }
        l.push((format!("  {} {}, {:.2} {}; {} {}", t("проверок опоры", "support checks"), st.sup_n, st.sup_ms, t("мс", "ms"), t("падающих тел", "falling bodies"), self.sim.bodies.len()), TXT));
        l.push((format!("{} {}", t("ЖИЗНЬ КОНВЕЯ:", "CONWAY LIFE:"), cy(st.life_ms, cpms)), 0x9ad0ff));
        l.push((format!("{} {}", t("ИИ СУЩЕСТВ:", "CREATURE AI:"), cy(self.ai_ms, cpms)), 0x9ad0ff));
        for k in 0..3 { let n = st.ai_n[k]; l.push((format!("  {}: {} x {:.1} {} = {:.0} {}", CRE_NAME[k][lg], n, st.ai_ns[k] / 1000. / n.max(1) as f32, t("мкс", "us"), st.ai_ns[k] / 1000., t("мкс", "us")), DIM)) }
        l.push((format!("{} {} ({} {})", t("ГЕНЕРАЦИЯ ЧАНКОВ:", "CHUNK GENERATION:"), cy(self.ms[3], cpms), self.gen_n, t("за кадр", "per frame")), 0x9ad0ff));
        l.push((format!("{} {}", t("СБОРКА КАДРА И UI:", "FRAME COMPOSE & UI:"), cy(self.ms[5] - self.present, cpms)), 0x9ad0ff));
        l.push((format!("{} {}", t("БЛИТТИНГ GDI (StretchDIBits):", "GDI BLIT (StretchDIBits):"), cy(self.present, cpms)), 0x9ad0ff));
        let w = l.iter().map(|s| tw(&s.0, 1)).max().unwrap_or(0) + 16;
        let (x, y) = (4, TOP + 16);
        self.cv.rect(x, y, w, l.len() as i32 * 12 + 10, 0x080a0c, 228);
        self.cv.frame(x, y, w, l.len() as i32 * 12 + 10, 0x3a6a8a);
        for (i, (s, c)) in l.iter().enumerate() { self.cv.text(x + 8, y + 5 + i as i32 * 12, s, *c, 1); }
    }
    fn menu_btns(&self) -> Vec<(i32, i32, i32, i32)> { (0..4).map(|i| ((self.cv.w - 280) / 2, self.cv.h / 2 - 40 + i * 38, 280, 28)).collect() }
    fn sl_bar(&self, i: usize) -> (i32, i32, i32, i32) { ((self.cv.w - 620) / 2 + 190, TOP + 50 + i as i32 * 54, 330, 12) }
    fn set_btns(&self) -> [(i32, i32, i32, i32); 3] { let (x0, y) = ((self.cv.w - 620) / 2, TOP + 50 + 6 * 54); [(x0, y, 220, 24), (x0 + 228, y, 240, 24), (x0 + 476, y, 144, 24)] }
    fn button(&mut self, b: (i32, i32, i32, i32), label: &str, sel: bool) {
        let hov = sel || inside(self.mouse.0, self.mouse.1, b);
        self.cv.rect(b.0, b.1, b.2, b.3, if hov { 0x2a3a48 } else { 0x161c22 }, 235);
        self.cv.frame(b.0, b.1, b.2, b.3, if hov { ACC } else { 0x3a4650 });
        let s = if b.3 >= 28 { 2 } else { 1 };
        self.cv.text(b.0 + (b.2 - tw(label, s)) / 2, b.1 + (b.3 - 10 * s) / 2, label, if hov { ACC } else { TXT }, s);
    }
    fn draw_menu(&mut self) {
        let (cw, ch) = (self.cv.w, self.cv.h);
        self.cv.rect(0, TOP, cw, ch - TOP, 0, 140);
        let t = self.l("СИМУЛЯЦИЯ РЕАЛЬНОСТИ", "REALITY SIMULATION");
        self.cv.text((cw - tw(t, 3)) / 2, ch / 2 - 130, t, ACC, 3);
        let s = self.l("песочница: от клеток Конвея до разрушаемого мира, людей и космоса", "sandbox: from Conway's cells to a destructible world, people and space");
        self.cv.text((cw - tw(s, 1)) / 2, ch / 2 - 90, s, DIM, 1);
        let labels = [if self.started { self.l("Продолжить", "Resume") } else { self.l("Начать", "Start") }, self.l("Настройки", "Settings"), self.l("Язык: RU", "Language: EN"), self.l("Выход", "Exit")];
        for (i, b) in self.menu_btns().into_iter().enumerate() { self.button(b, labels[i], i == self.menu_sel) }
        let s = self.l("Стрелки + Enter или мышь.  Esc - вернуться в игру", "Arrows + Enter or mouse.  Esc - back to the game");
        self.cv.text((cw - tw(s, 1)) / 2, ch / 2 + 120, s, DIM, 1);
    }
    fn sl_info(&self, i: usize) -> String {
        let (q, ru) = (self.q, self.ru);
        let on = |b: bool| match (b, ru) { (true, true) => "вкл", (false, true) => "выкл", (true, false) => "on", _ => "off" };
        let lv = LEVELS[q.level as usize][!ru as usize];
        match (i, ru) {
            (0, true) => format!("радиус мира {} м, память мира {:.1} МБ, журнал {} событий, до {} существ", q.radius / S, q.mem_mb, q.hist, q.max_ent),
            (0, false) => format!("world radius {} m, world memory {:.1} MB, log of {} events, up to {} creatures", q.radius / S, q.mem_mb, q.hist, q.max_ent),
            (1, true) => format!("тик {:.0} Гц, уровень {} - {}, глубина обрушений {} вокс.", q.tick_hz, q.level, lv, q.bfs),
            (1, false) => format!("tick {:.0} Hz, level {} - {}, collapse depth {} vox", q.tick_hz, q.level, lv, q.bfs),
            (2, true) => format!("разрешение {:.0}%, дальность {:.0} м, тени {}, текстуры {}, прозрачность {}, частицы {}", q.res * 100., q.view / SF, on(q.shadows), on(q.tex), on(q.water), q.parts),
            (2, false) => format!("resolution {:.0}%, view {:.0} m, shadows {}, textures {}, transparency {}, particles {}", q.res * 100., q.view / SF, on(q.shadows), on(q.tex), on(q.water), q.parts),
            (3, true) => format!("чанков за кадр {}, октав рельефа {}, уникальность объектов {:.0}%", q.chunks, q.oct, q.variety * 100.),
            (3, false) => format!("chunks per frame {}, terrain octaves {}, object variety {:.0}%", q.chunks, q.oct, q.variety * 100.),
            (4, true) => format!("цель {:.0} кадров/с = бюджет {:.1} мс на весь кадр", q.fps, 1000. / q.fps),
            (4, false) => format!("target {:.0} frames/s = {:.1} ms budget per frame", q.fps, 1000. / q.fps),
            (_, true) => format!("вид: {}   (1-33 текст, 34-66 2D-срез, 67-100 3D)", MODES[q.mode as usize][0]),
            (_, false) => format!("view: {}   (1-33 text, 34-66 2D slice, 67-100 3D)", MODES[q.mode as usize][1]),
        }
    }
    fn draw_settings(&mut self) {
        let (cw, ch, lg) = (self.cv.w, self.cv.h, !self.ru as usize);
        let x0 = (cw - 620) / 2;
        self.cv.rect(0, TOP, cw, ch - TOP, 0, 150);
        self.cv.rect(x0 - 12, TOP + 6, 644, ch - TOP - 28, PANEL, 240);
        self.cv.frame(x0 - 12, TOP + 6, 644, ch - TOP - 28, 0x3a4650);
        let t = self.l("НАСТРОЙКИ СИМУЛЯЦИИ", "SIMULATION SETTINGS");
        self.cv.text((cw - tw(t, 2)) / 2, TOP + 14, t, ACC, 2);
        for i in 0..6 {
            let (b, sel) = (self.sl_bar(i), i == self.sel);
            let c = if sel { ACC } else { TXT };
            self.cv.text(x0, b.1 + 1, &format!("{}. {}", i + 1, SL_NAME[i][lg]), c, 1);
            self.cv.rect(b.0, b.1, b.2, b.3, 0x2a3238, 255);
            self.cv.rect(b.0, b.1, b.2 * self.sl[i] / 100, b.3, if sel { ACC } else { 0x4a90c0 }, 255);
            if sel { self.cv.frame(b.0 - 2, b.1 - 2, b.2 + 4, b.3 + 4, ACC) }
            self.cv.text(b.0 + b.2 + 10, b.1 + 1, &format!("{}%", self.sl[i]), c, 1);
            let info = self.sl_info(i);
            self.cv.text(x0, b.1 + 18, &info, DIM, 1);
        }
        let [bp, bd, bb] = self.set_btns();
        let pl = PLANETS[self.wgen.pl as usize];
        let (lp, ld) = (if self.ru { format!("Планета: {}", pl.ru) } else { format!("Planet: {}", pl.en) }, if self.ru { format!("Инструмент отладки CPU: {}", if self.prof { "вкл" } else { "выкл" }) } else { format!("CPU debug tool: {}", if self.prof { "on" } else { "off" }) });
        self.button(bp, &lp, false);
        self.button(bd, &ld, self.prof);
        let back = self.l("Назад", "Back");
        self.button(bb, back, false);
        let s = self.l("[1-6] выбор  [Влево/Вправо] значение (Shift = x10)  [L] язык  [F3] отладка  [Esc] назад", "[1-6] select  [Left/Right] value (Shift = x10)  [L] language  [F3] debug  [Esc] back");
        self.cv.text((cw - tw(s, 1)) / 2, ch - 38, s, DIM, 1);
    }
    fn draw_editor(&mut self) {
        let (cw, ch) = (self.cv.w, self.cv.h);
        let (x, y, w, h) = (16, ch / 2 - 40, cw - 32, ch / 2 + 16);
        self.cv.rect(x, y, w, h, PANEL, 235);
        self.cv.frame(x, y, w, h, ACC);
        let t = self.l("РЕДАКТОР МИРА   [Enter] выполнить   [Tab/Esc] закрыть", "WORLD EDITOR   [Enter] run   [Tab/Esc] close");
        self.cv.text(x + 8, y + 6, t, ACC, 1);
        let e = self.l("Примеры: остров с вулканом и лесом | стада и волки на равнине | луна | марс | пояс астероидов | если камень рядом лава то лава 5%",
            "Examples: volcanic island with forest | herds and wolves on plains | moon | mars | asteroid belt | if stone near lava then lava 5%");
        self.cv.text(x + 8, y + 20, e, DIM, 1);
        let n = ((h - 58) / 12) as usize;
        let start = self.out.len().saturating_sub(n);
        for (i, s) in self.out[start..].iter().enumerate() { self.cv.text(x + 8, y + 36 + i as i32 * 12, s, if s.starts_with('>') { ACC } else { TXT }, 1); }
        let cur = if (self.time * 2.) as i32 % 2 == 0 { "_" } else { " " };
        self.cv.rect(x + 4, y + h - 18, w - 8, 14, 0x1e262e, 255);
        self.cv.text(x + 8, y + h - 16, &format!("> {}{}", self.line, cur), TXT, 1);
    }
    fn draw_help(&mut self) {
        let ru = ["УПРАВЛЕНИЕ", "WASD - движение, Space/C - вверх/вниз (2D: глубина среза), Shift - быстрее; бог может улететь в космос",
            "ПКМ (зажать) - обзор мышью в 3D;  ЛКМ - инструмент под курсором;  I - осмотр (инспектор), Backspace - закрыть",
            "Колесо - инструмент: Осмотр / Взрыв / Материал / Ластик / Огонь / Клетки / Травоядное / Хищник / Человек",
            "Q/E - материал;  стрелки вверх/вниз - кисть;  F - вселиться в человека / выйти из тела",
            "1-6 - ползунок, стрелки влево/вправо - значение (Shift = x10);  F3 - отладка CPU",
            "Tab - редактор мира и правил (в т.ч. луна, марс, пояс астероидов);  P - пауза;  Esc - меню", "",
            "УРОВНИ (ЦП): 0 - клеточная жизнь, 1 - физика и обрушения, 2 - химия и огонь,",
            "3 - травоядные и хищники, 4 - люди (рубят лес, строят хижины). Мир: 2 вокселя на метр.",
            "Ползунок 6 (Мир): 1-33 текст, 34-66 2D-срез, 67-100 3D. Все режимы показывают один мир."];
        let en = ["CONTROLS", "WASD - move, Space/C - up/down (2D: slice depth), Shift - faster; the god can fly up to space",
            "Hold RMB - mouse look in 3D;  LMB - tool under cursor;  I - inspect, Backspace - close inspector",
            "Wheel - tool: Inspect / Blast / Material / Eraser / Fire / Cells / Herbivore / Predator / Human",
            "Q/E - material;  Up/Down arrows - brush;  F - incarnate as a human / leave the body",
            "1-6 - slider, Left/Right arrows - value (Shift = x10);  F3 - CPU debug",
            "Tab - world & rule editor (incl. moon, mars, asteroid belt);  P - pause;  Esc - menu", "",
            "LEVELS (CPU): 0 - cellular life, 1 - physics & collapses, 2 - chemistry & fire,",
            "3 - herbivores and predators, 4 - people (chop trees, build huts). World: 2 voxels per metre.",
            "Slider 6 (World): 1-33 text, 34-66 2D slice, 67-100 3D. All views show the same world."];
        let lines = if self.ru { ru } else { en };
        let w = lines.iter().map(|s| tw(s, 1)).max().unwrap_or(0) + 24;
        let (x, y) = ((self.cv.w - w) / 2, TOP + 60);
        self.cv.rect(x, y, w, lines.len() as i32 * 13 + 16, PANEL, 240);
        self.cv.frame(x, y, w, lines.len() as i32 * 13 + 16, ACC);
        for (i, s) in lines.iter().enumerate() { self.cv.text(x + 12, y + 8 + i as i32 * 13, s, if i == 0 { ACC } else { TXT }, 1); }
    }
}

// ─── MAIN: цикл окна ────────────────────────────────────────────────────────────────────────────
#[cfg(windows)]
fn main() {
    let mut win = win::Window::new("Реальность / Reality");
    let (mut g, mut inp, mut evs) = (Game::new(win.w, win.hgt), Input::new(), Vec::new());
    while win.pump(&mut evs) {
        inp.apply(std::mem::take(&mut evs));
        inp.look = win.look(inp.rmb && g.wants_look());
        g.frame(&mut inp);
        if g.quit { win.close(); g.quit = false; inp.end_frame(); continue }
        let t = Instant::now();
        win.present(&g.cv.px, g.cv.w, g.cv.h);
        g.end_frame(t.elapsed().as_secs_f32() * 1000., true);
        inp.end_frame();
    }
}
