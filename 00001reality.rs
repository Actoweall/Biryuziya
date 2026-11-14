// ═══════════════════════════════════════════════════════════════════════════════════════════════
//  РЕАЛЬНОСТЬ / REALITY — песочница-симулятор реальности: от клеток Конвея до разрушаемого мира.
//  Один файл, чистый Rust (edition 2024), без крейтов. Платформа: Windows (Win32 + GDI через FFI).
//  Сборка:  rustc -C opt-level=3 --edition 2024 reality.rs      Запуск: reality.exe
//  Разделы: PLATFORM · INPUT · FONT · UI · CONFIG · MATERIALS · WORLD · GEN · SIM · EDITOR · RENDER · GAME · MAIN
//  Правило для итераций: каждый раздел общается с другими только через свои структуры/функции.
// ═══════════════════════════════════════════════════════════════════════════════════════════════
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::collections::{HashMap, HashSet, VecDeque};
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

// ─── CONFIG: 6 ползунков (1..100%) → конкретные параметры движка ────────────────────────────────
const SL_NAME: [[&str; 2]; 6] = [["ОЗУ", "RAM"], ["ЦП", "CPU"], ["ГП", "GPU"], ["Накопитель", "Storage"], ["Кадры/с", "FPS"], ["Мир", "World"]];
const LEVELS: [[&str; 2]; 5] = [["Клеточная жизнь", "Cellular life"], ["Физика", "Physics"], ["Химия и огонь", "Chemistry & fire"], ["Организмы*", "Organisms*"], ["Общество*", "Society*"]];
const MODES: [[&str; 2]; 3] = [["Текст", "Text"], ["2D", "2D"], ["3D", "3D"]];
const TOOLS: [[&str; 2]; 5] = [["Взрыв", "Blast"], ["Материал", "Material"], ["Ластик", "Eraser"], ["Огонь", "Fire"], ["Жизнь", "Life"]];
#[derive(Clone, Copy)]
struct Q { radius: i32, mem_mb: f32, hist: usize, tick_hz: f32, level: u8, bfs: usize, nsup: usize, res: f32, view: f32, shadows: bool, tex: bool, water: bool, parts: usize, txt: i32, depth2d: i32, mono: bool, chunks: usize, oct: u32, variety: f32, fps: f32, mode: u8 }
fn quality(s: &[i32; 6]) -> Q {
    let f = |i: usize| s[i] as f32 / 100.;
    let g = f(2);
    Q {
        radius: 16 + (s[0] - 1) * 96 / 99, mem_mb: 0.25 * 2f32.powf(f(0) * 8.), hist: 10 + s[0] as usize * 2,                     // ОЗУ
        tick_hz: 4. + f(1) * 56., level: match s[1] { 0..=15 => 0, 16..=40 => 1, 41..=70 => 2, 71..=90 => 3, _ => 4 },              // ЦП
        bfs: 400 + s[1] as usize * 200, nsup: 4 + s[1] as usize / 2,
        res: 0.15 + 0.85 * g, view: 16. + 144. * g, shadows: g > 0.5, tex: g > 0.25, water: g > 0.65, parts: (g * 3000.) as usize, // ГП
        txt: if g < 0.34 { 3 } else if g < 0.67 { 2 } else { 1 }, depth2d: (g * 12.) as i32, mono: g < 0.2,
        chunks: 1 + s[3] as usize * 31 / 100, oct: 1 + s[3] as u32 * 5 / 100, variety: f(3),                                      // Накопитель
        fps: (s[4] as f32 * 2.4).max(10.),                                                                                        // Кадры/с
        mode: if s[5] < 34 { 0 } else if s[5] < 67 { 1 } else { 2 },                                                             // Мир
    }
}

// ─── MATERIALS: вещества мира и их физические свойства ──────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum K { Air, Solid, Powder, Liquid, Gas, Fire, Life }
struct Mat { ru: &'static str, en: &'static str, sym: char, col: u32, k: K, dens: u8, stems: &'static str }
const fn mt(ru: &'static str, en: &'static str, sym: char, col: u32, k: K, dens: u8, stems: &'static str) -> Mat { Mat { ru, en, sym, col, k, dens, stems } }
const MATS: [Mat; 21] = [
    mt("Воздух", "Air", ' ', 0, K::Air, 0, "возд|пуст|ничт|air|empty|nothing"),
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
];
const AIR: u8 = 0; const BEDROCK: u8 = 1; const STONE: u8 = 2; const DIRT: u8 = 3; const GRASS: u8 = 4; const SAND: u8 = 5; const WATER: u8 = 6;
const LAVA: u8 = 7; const WOOD: u8 = 8; const LEAF: u8 = 9; const FIRE: u8 = 10; const SMOKE: u8 = 11; const STEAM: u8 = 12; const ICE: u8 = 13;
const SNOW: u8 = 14; const GLASS: u8 = 15; const BRICK: u8 = 17; const METAL: u8 = 18; const GRAVEL: u8 = 19; const LIFE: u8 = 20;
const VOID: u8 = 255; // незагруженный чанк: твёрдый для физики, туман для рендера
fn kind(m: u8) -> K { MATS.get(m as usize).map_or(K::Solid, |x| x.k) }
fn dens(m: u8) -> u8 { MATS.get(m as usize).map_or(255, |x| x.dens) }
fn glow(m: u8) -> bool { matches!(m, FIRE | LAVA | LIFE) }
fn surf(m: u8) -> bool { !matches!(kind(m), K::Air | K::Gas | K::Fire) }

// ─── WORLD: кольцевой воксельный буфер 256×64×256 вокруг игрока, чанки 16×64×16, память изменений ─
const W: usize = 256; const H: usize = 64; const WM: i32 = 255; const NONE: (i32, i32) = (i32::MIN, i32::MIN);
const N6: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)]; // «вниз» последним: поиск опоры сначала ныряет к земле
fn vi(x: i32, y: i32, z: i32) -> usize { ((z & WM) as usize * W + (x & WM) as usize) * H + y as usize }
fn ci(x: i32, z: i32) -> usize { (z & WM) as usize * W + (x & WM) as usize }
fn si(cx: i32, cz: i32) -> usize { ((cz & 15) * 16 + (cx & 15)) as usize }
pub struct World { v: Vec<u8>, st: Vec<u8>, top: Vec<u8>, slot: Vec<(i32, i32)>, modf: Vec<bool>, dirty: Vec<bool>, ndirty: Vec<bool>, mem: HashMap<(i32, i32), Vec<u8>>, memq: VecDeque<(i32, i32)>, mem_bytes: usize }
impl World {
    fn new() -> Self {
        World { v: vec![0; W * W * H], st: vec![0; W * W * H], top: vec![0; W * W], slot: vec![NONE; 256], modf: vec![false; 256], dirty: vec![false; 1024], ndirty: vec![false; 1024], mem: HashMap::new(), memq: VecDeque::new(), mem_bytes: 0 }
    }
    fn reset(&mut self) { self.slot.fill(NONE); self.modf.fill(false); self.mem.clear(); self.memq.clear(); self.mem_bytes = 0 }
    fn loaded(&self, x: i32, z: i32) -> bool { self.slot[si(x >> 4, z >> 4)] == (x >> 4, z >> 4) }
    fn get(&self, x: i32, y: i32, z: i32) -> u8 { if y < 0 { BEDROCK } else if y >= H as i32 { AIR } else if !self.loaded(x, z) { VOID } else { self.v[vi(x, y, z)] } }
    fn top(&self, x: i32, z: i32) -> i32 { self.top[ci(x, z)] as i32 }
    fn wake(&mut self, x: i32, y: i32, z: i32) { if (0..H as i32).contains(&y) && self.loaded(x, z) { self.ndirty[si(x >> 4, z >> 4) * 4 + (y >> 4) as usize] = true } }
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
        self.modf[si(x >> 4, z >> 4)] = true;
        true
    }
    /// Выгружаемый изменённый чанк сжимается (RLE) в «память мира»; старые воспоминания вытесняются по лимиту ОЗУ.
    fn store(&mut self, s: usize, cap: usize) {
        let (cx, cz) = self.slot[s];
        let mut out: Vec<u8> = Vec::new();
        for z in cz * 16..cz * 16 + 16 { for x in cx * 16..cx * 16 + 16 {
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
        for z in cz * 16..cz * 16 + 16 { for x in cx * 16..cx * 16 + 16 { let b = vi(x, 0, z); for y in 0..H { self.v[b + y] = it.next().unwrap_or(AIR) } } }
        true
    }
}

// ─── GEN: процедурная генерация чанков по параметрам мира (Gen) и ползунку накопителя ──────────
#[derive(Clone)]
struct Gen { seed: u64, sea: i32, base: f32, amp: f32, forest: f32, snow: bool, desert: bool, volcano: bool, life: f32, city: f32, caves: bool, ruins: bool }
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
    fn base(seed: u64) -> Gen { Gen { seed, sea: 22, base: 25., amp: 12., forest: 0.45, snow: false, desert: false, volcano: false, life: 0.35, city: 0., caves: false, ruins: false } }
    /// Высота рельефа и расстояние до ближайшего жерла вулкана.
    fn height(&self, x: i32, z: i32, oct: u32) -> (i32, f32) {
        let mut h = self.base + (fbm(x as f32 / 72., z as f32 / 72., oct, self.seed) - 0.5) * 2.4 * self.amp;
        let mut vd = 1e9;
        if self.volcano {
            let (gx, gz) = (x.div_euclid(160), z.div_euclid(160));
            let (vx, vz) = (gx * 160 + 40 + (hf(gx, gz, 1, self.seed) * 80.) as i32, gz * 160 + 40 + (hf(gx, gz, 2, self.seed) * 80.) as i32);
            vd = (((x - vx).pow(2) + (z - vz).pow(2)) as f32).sqrt();
            if vd < 34. { h = h.max(self.base) + 34. - vd; if vd < 5. { h -= (5. - vd) * 2.5 } }
        }
        ((h as i32).clamp(3, H as i32 - 6), vd)
    }
}
fn gen_chunk(w: &mut World, g: &Gen, q: &Q, cx: i32, cz: i32) {
    let s = si(cx, cz);
    if w.slot[s] != NONE && w.modf[s] { w.store(s, (q.mem_mb * 1_048_576.) as usize) }
    w.slot[s] = (cx, cz);
    let restored = w.restore(cx, cz);
    let col = |lx: i32, lz: i32| (cx * 16 + lx, cz * 16 + lz);
    if !restored {
        let mut hs = [0i32; 256];
        for lz in 0..16 { for lx in 0..16 {
            let (x, z) = col(lx, lz);
            let (h, vd) = g.height(x, z, q.oct);
            hs[(lz * 16 + lx) as usize] = h;
            let top = if vd < 5. { LAVA } else if g.desert || h <= g.sea + 1 { if h < g.sea - 3 && hf(x, z, 1, g.seed) < 0.3 { GRAVEL } else { SAND } } else if g.snow || h > 46 { SNOW } else { GRASS };
            let b = vi(x, 0, z);
            for y in 0..H as i32 {
                w.v[b + y as usize] = if y == 0 { BEDROCK } else if y < h - 3 {
                    if g.caves && y > 2 && noise(x as f32 / 14., y as f32 / 9., z as f32 / 14., g.seed + 9) > 0.72 { AIR }
                    else if q.variety > 0.6 && hf(x, y, z, g.seed + 5) < 0.004 { METAL } else { STONE }
                } else if y < h { if vd < 5. { LAVA } else if top == SAND || top == GRAVEL || g.desert { SAND } else { DIRT } }
                else if y == h { top } else if y <= g.sea { if g.snow && y == g.sea { ICE } else { WATER } } else { AIR };
            }
        } }
        for lz in 2..14 { for lx in 2..14 { // деревья: форма и высота разнообразнее при высоком «Накопителе»
            let (x, z) = col(lx, lz);
            let h = hs[(lz * 16 + lx) as usize];
            if !matches!(w.v[vi(x, h, z)], GRASS | SNOW) || h <= g.sea + 1 || h > H as i32 - 10 || hf(x, z, 7, g.seed) >= g.forest * 0.03 { continue }
            let th = 4 + if q.variety > 0.4 { (hash(x, z, 8, g.seed) % 3) as i32 } else { 0 };
            for y in h + 1..=h + th { w.v[vi(x, y, z)] = WOOD }
            for dy in -2..=2i32 { for dz in -2..=2i32 { for dx in -2..=2i32 {
                let (lx2, y, lz2) = (x + dx, h + th + dy, z + dz);
                if dx * dx + dy * dy + dz * dz > 5 || y >= H as i32 - 1 || (q.variety > 0.7 && hf(lx2, y, lz2, g.seed) < 0.15) { continue }
                let i = vi(lx2, y, lz2);
                if w.v[i] == AIR { w.v[i] = LEAF }
            } } }
        } }
        let ruin = g.ruins && hf(cx, cz, 5, g.seed) < 0.6;
        if (g.city > 0. && hf(cx, cz, 3, g.seed) < g.city * 0.45) || (g.ruins && hf(cx, cz, 4, g.seed) < 0.3) { // дома / руины
            let h0 = hs[8 * 16 + 8];
            let (sx, sz) = (6 + (hash(cx, cz, 6, g.seed) % 4) as i32, 6 + (hash(cx, cz, 7, g.seed) % 4) as i32);
            if h0 > g.sea && h0 < H as i32 - 16 {
                for lz in 3..3 + sz { for lx in 3..3 + sx {
                    let (x, z) = col(lx, lz);
                    let edge = lx == 3 || lz == 3 || lx == 2 + sx || lz == 2 + sz;
                    for y in 1..H as i32 {
                        let m = if y <= h0 { if y > hs[(lz * 16 + lx) as usize] { STONE } else { continue } }
                            else if y <= h0 + 4 { if !edge || (lz == 3 && lx == 3 + sx / 2 && y <= h0 + 2) { AIR } else if y == h0 + 2 && (lx + lz) % 3 == 0 { GLASS } else { BRICK } }
                            else if y == h0 + 5 && !ruin { WOOD } else if y <= h0 + 14 { AIR } else { continue };
                        w.v[vi(x, y, z)] = if ruin && m != AIR && y > h0 && hf(x, y, z, g.seed + 3) < 0.45 { if y == h0 + 1 { GRAVEL } else { AIR } } else { m };
                    }
                } }
            }
        }
        if g.life > 0. { for lz in 0..16 { for lx in 0..16 { // очаги клеточной жизни (Конвей)
            let (x, z) = col(lx, lz);
            let h = (0..H as i32).rev().find(|&y| surf(w.v[vi(x, y, z)])).unwrap_or(0);
            if matches!(w.v[vi(x, h, z)], GRASS | DIRT | SAND | SNOW) && h + 1 < H as i32 && w.v[vi(x, h + 1, z)] == AIR
                && noise(x as f32 / 10., z as f32 / 10., 0., g.seed + 3) > 1. - g.life * 0.5 && hf(x, z, 9, g.seed) < 0.4 { w.v[vi(x, h + 1, z)] = LIFE }
        } } }
    }
    for lz in 0..16 { for lx in 0..16 { let (x, z) = col(lx, lz); w.top[ci(x, z)] = (0..H as i32).rev().find(|&y| surf(w.v[vi(x, y, z)])).unwrap_or(0) as u8 } }
    w.modf[s] = restored;
    for k in 0..4 { w.ndirty[s * 4 + k] = true }
}

// ─── SIM: уровни симуляции. 0 — жизнь Конвея, 1 — сыпучие/жидкости/газы и обрушения, 2 — правила-реакции ─
struct Rule { a: u8, b: u8, c: u8, p: f32 } // «A (рядом с B) → C с шансом p за тик»; b = VOID — без соседа
struct Body { vox: Vec<([i32; 3], u8)>, v: f32, acc: f32, fell: i32 }
pub struct Sim { w: World, rules: Vec<Rule>, by_a: Vec<Vec<usize>>, by_b: Vec<Vec<usize>>, bodies: Vec<Body>, claimed: HashSet<[i32; 3]>, supq: VecDeque<[i32; 3]>, tick: u32, rng: u64, fx: Vec<([f32; 3], u32)>, ev: Vec<(String, String)> }
impl Sim {
    fn new() -> Self {
        let mut s = Sim { w: World::new(), rules: vec![], by_a: vec![], by_b: vec![], bodies: vec![], claimed: HashSet::new(), supq: VecDeque::new(), tick: 0, rng: 0x2545_F491_4F6C_DD1D, fx: vec![], ev: vec![] };
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
    }
    /// Уровни 1+: обход «проснувшихся» секций 16³ в радиусе — падение, течение, всплытие, реакции, обрушения.
    fn phys_tick(&mut self, c: (i32, i32), q: &Q) {
        self.tick = self.tick.wrapping_add(1);
        let t = (self.tick % 255 + 1) as u8;
        std::mem::swap(&mut self.w.dirty, &mut self.w.ndirty);
        self.w.ndirty.fill(false);
        let (pcx, pcz, rc) = (c.0 >> 4, c.1 >> 4, ((q.radius >> 4) + 1).min(7));
        let mut secs = vec![];
        for sy in 0..4 { for cz in pcz - rc..=pcz + rc { for cx in pcx - rc..=pcx + rc {
            let s = si(cx, cz);
            if self.w.slot[s] == (cx, cz) && self.w.dirty[s * 4 + sy] { self.w.dirty[s * 4 + sy] = false; secs.push((cx, cz, sy as i32)) }
        } } }
        for i in 0..1024 { if self.w.dirty[i] { self.w.ndirty[i] = true } } // вне радиуса — ждут игрока
        for &(cx, cz, sy) in &secs { for y in (sy * 16).max(1)..sy * 16 + 16 { for z in cz * 16..cz * 16 + 16 { for x in cx * 16..cx * 16 + 16 { self.cell(x, y, z, t, q.level, false) } } } }
        for &(cx, cz, sy) in secs.iter().rev() { for y in ((sy * 16).max(1)..sy * 16 + 16).rev() { for z in cz * 16..cz * 16 + 16 { for x in cx * 16..cx * 16 + 16 { self.cell(x, y, z, t, q.level, true) } } } }
        self.bodies_tick();
        let mut seen = HashSet::new();
        let mut budget = q.nsup;
        while budget > 0 {
            let Some(p) = self.supq.pop_front() else { break };
            if seen.contains(&p) || kind(self.w.get(p[0], p[1], p[2])) != K::Solid { continue }
            self.support(p, q.bfs, &mut seen);
            budget -= 1;
        }
    }
    fn cell(&mut self, x: i32, y: i32, z: i32, t: u8, lvl: u8, gas: bool) {
        let i = vi(x, y, z);
        let m = self.w.v[i];
        if m == AIR || self.w.st[i] == t { return }
        let k = kind(m);
        if gas { if k == K::Gas { self.flow(x, y, z, m, t, 2); } return }
        if lvl >= 2 && self.react(x, y, z, m, t) { return }
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
        if mi >= MATS.len() { return false }
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
        let mut bs = std::mem::take(&mut self.bodies);
        bs.retain_mut(|b| {
            b.v = (b.v + 0.08).min(2.);
            b.acc += b.v;
            while b.acc >= 1. {
                b.acc -= 1.;
                for &(p, _) in &b.vox { self.w.set(p[0], p[1], p[2], AIR); }
                let free = b.vox.iter().all(|&(p, _)| p[1] > 1 && dens(self.w.get(p[0], p[1] - 1, p[2])) < 20);
                if free { for (p, _) in b.vox.iter_mut() { p[1] -= 1 } b.fell += 1 }
                for &(p, m) in &b.vox { self.w.set(p[0], p[1], p[2], m); }
                if !free {
                    if b.fell > 2 { // удар о землю: часть массы рассыпается в щебень
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
        let n = set.len();
        let vox: Vec<_> = set.iter().map(|&p| (p, self.w.get(p[0], p[1], p[2]))).collect();
        seen.extend(set);
        if grounded || self.bodies.len() > 64 { return }
        if n >= 12 { self.ev.push((format!("Обрушение: {} вокс. потеряли опору", n), format!("Collapse: {} voxels lost support", n))) }
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
                if self.chance(0.25) { self.fx.push(([p[0] as f32 + 0.5, p[1] as f32 + 0.5, p[2] as f32 + 0.5], MATS[m as usize].col)) }
                let fire = lvl >= 2 && d2 >= (r - 1) * (r - 1) && self.chance(0.15);
                self.w.set(p[0], p[1], p[2], if fire { FIRE } else { AIR });
            } else if d2 <= (r + 1) * (r + 1) && kind(m) == K::Solid && m != VOID { shell.push(p) }
        } } }
        if lvl >= 1 { self.supq.extend(shell); while self.supq.len() > 4096 { self.supq.pop_front(); } }
    }
}

// ─── EDITOR: текстовый запрос мира (RU/EN) и мини-язык правил «если A [рядом B] то C N%» ──────────
const DEFAULT_PROMPT: &str = "острова, горы, лес, жизнь";
const DEFAULT_RULES: [&str; 16] = [
    "если дерево рядом огонь то огонь 20%", "если листва рядом огонь то огонь 35%", "если трава рядом огонь то огонь 25%",
    "если жизнь рядом огонь то огонь 40%", "если огонь то дым 7%", "если дым то воздух 3%", "если вода рядом огонь то пар 50%",
    "если пар то воздух 2%", "если вода рядом лава то пар 20%", "если лава рядом пар то камень 30%", "если песок рядом лава то стекло 2%",
    "если лед рядом огонь то вода 25%", "если снег рядом огонь то вода 35%", "если лед рядом лава то вода 30%",
    "если трава рядом лава то огонь 20%", "если дерево рядом лава то огонь 30%",
];
const FEATS: [(&str, &str, &str); 12] = [
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
/// Запрос на естественном языке → параметры генератора + список распознанных понятий.
fn parse_prompt(s: &str, seed: u64) -> (Gen, Vec<usize>) {
    let mut f = vec![];
    for w in words(s) { if let Some(i) = FEATS.iter().position(|x| stem(x.0, &w)) { if !f.contains(&i) { f.push(i) } } }
    let mut g = Gen::base(seed);
    for i in 0..FEATS.len() {
        if !f.contains(&i) { continue }
        match i {
            0 => g.city = 0.7, 1 => g.city = g.city.max(0.6), 2 => { g.amp = 26.; g.base += 5. } 3 => g.amp = 3., 4 => g.forest = 1.,
            5 => g.snow = true, 6 => { g.desert = true; g.forest *= 0.15; g.sea = 10 } 7 => g.volcano = true, 8 => g.life = 0.9,
            9 => { g.sea = 30; g.base = g.base.min(26.) } 10 => g.caves = true, _ => { g.ruins = true; g.city = g.city.max(0.5) }
        }
    }
    (g, f)
}

// ─── RENDER: текст (сверху), 2D (вертикальный срез), 3D (воксельный рейкаст на всех ядрах CPU) ────
type V3 = [f32; 3];
const FOV: f32 = 0.7; const SUN: V3 = [0.4, 0.8, 0.45];
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
/// Проход луча по вокселям (Amanatides–Woo). pass — вещество, сквозь которое луч проходит как сквозь воздух.
fn dda(w: &World, o: V3, d: V3, maxd: f32, pass: u8) -> Option<Hit> {
    let mut p = [o[0].floor() as i32, o[1].floor() as i32, o[2].floor() as i32];
    let (mut st, mut tm, mut td) = ([0i32; 3], [f32::INFINITY; 3], [f32::INFINITY; 3]);
    for a in 0..3 {
        if d[a] > 0. { st[a] = 1; td[a] = 1. / d[a]; tm[a] = (p[a] as f32 + 1. - o[a]) * td[a] }
        else if d[a] < 0. { st[a] = -1; td[a] = -1. / d[a]; tm[a] = (o[a] - p[a] as f32) * td[a] }
    }
    let (mut t, mut ax, mut prev) = (0f32, 1usize, p);
    loop {
        let m = w.get(p[0], p[1], p[2]);
        if m != AIR && m != pass { return Some(Hit { p, prev, m, t, ax, s: st[ax] }) }
        if t > maxd || (p[1] >= H as i32 && d[1] >= 0.) { return None }
        prev = p;
        ax = if tm[0] < tm[1] { if tm[0] < tm[2] { 0 } else { 2 } } else if tm[1] < tm[2] { 1 } else { 2 };
        t = tm[ax];
        tm[ax] += td[ax];
        p[ax] += st[ax];
    }
}
fn sky(d: V3) -> u32 { if d[1] < 0. { mix(0xa8c4e0, 0x6a8098, (-d[1]).min(1.)) } else { mix(0xb8d4f0, 0x2f63bf, d[1].sqrt()) } }
fn tex(c: u32, x: i32, y: i32, z: i32) -> u32 { mul(c, 0.88 + hf(x, y, z, 77) * 0.24) }
fn shade(w: &World, o: V3, d: V3, q: &Q, pass0: u8) -> (u32, f32) {
    let Some(mut h) = dda(w, o, d, q.view, pass0) else { return (sky(d), 1e9) };
    let mut tint = None;
    if q.water && matches!(h.m, WATER | GLASS | ICE | STEAM | SMOKE) {
        let tt = (MATS[h.m as usize].col, if h.m == WATER { 0.6 } else { 0.4 });
        tint = Some(tt);
        match dda(w, add(o, sc(d, h.t + 0.01)), d, q.view - h.t, h.m) { Some(h2) => h = Hit { t: h.t + h2.t + 0.01, ..h2 }, None => return (mix(sky(d), tt.0, tt.1), h.t) }
    }
    let mut c = if h.m == VOID { mix(sky(d), 0x9fb3c8, 0.5) } else { MATS[h.m as usize].col };
    if h.m != VOID {
        if q.tex { c = tex(c, h.p[0], h.p[1], h.p[2]) }
        let mut l = [0.78, if h.s < 0 { 1. } else { 0.5 }, 0.64][h.ax];
        if glow(h.m) { l = 1.2 } else if q.shadows {
            let mut sp = add(o, sc(d, h.t));
            sp[h.ax] -= h.s as f32 * 0.02;
            if dda(w, sp, SUN, 48., AIR).is_some_and(|s| s.m != VOID) { l *= 0.55 }
        }
        c = mul(c, l);
        c = mix(c, sky(d), ((h.t / q.view - 0.55) / 0.45).clamp(0., 1.));
    }
    if let Some((tc, a)) = tint { c = mix(c, tc, a) }
    (c, h.t)
}
/// Параллельный проход по строкам кадра на всех ядрах (std::thread::scope).
fn par(buf: &mut [u32], zb: &mut [f32], ww: usize, f: &(dyn Fn(usize, usize) -> (u32, f32) + Sync)) {
    let n = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows = (buf.len() / ww).div_ceil(n).max(1) * ww;
    std::thread::scope(|s| {
        for (k, (b, z)) in buf.chunks_mut(rows).zip(zb.chunks_mut(rows)).enumerate() {
            s.spawn(move || for i in 0..b.len() { let g = k * rows + i; (b[i], z[i]) = f(g % ww, g / ww) });
        }
    });
}
fn render3d(w: &World, v: &View, q: &Q, buf: &mut [u32], zb: &mut [f32], ww: usize, wh: usize) {
    let (f, r, u) = basis(v.yaw, v.pitch);
    let asp = ww as f32 / wh as f32;
    let m0 = w.get(v.p[0].floor() as i32, v.p[1].floor() as i32, v.p[2].floor() as i32); // камера под водой / в дыму
    let p0 = if matches!(kind(m0), K::Liquid | K::Gas) { m0 } else { AIR };
    par(buf, zb, ww, &|x, y| {
        let (sx, sy) = ((2. * (x as f32 + 0.5) / ww as f32 - 1.) * FOV * asp, (1. - 2. * (y as f32 + 0.5) / wh as f32) * FOV);
        let (c, t) = shade(w, v.p, nrm(add(f, add(sc(r, sx), sc(u, sy)))), q, p0);
        (if p0 == AIR { c } else { mix(c, MATS[p0 as usize].col, 0.45) }, t)
    });
}
fn render2d(w: &World, v: &View, q: &Q, buf: &mut [u32], zb: &mut [f32], ww: usize, wh: usize, ppv: f32) {
    let z = v.p[2].floor() as i32;
    par(buf, zb, ww, &|px, py| {
        let (fx, fy) = (v.p[0] + (px as f32 - ww as f32 / 2.) / ppv, v.p[1] + (wh as f32 / 2. - py as f32) / ppv);
        let (x, y) = (fx.floor() as i32, fy.floor() as i32);
        let m = w.get(x, y, z);
        let c = if m == AIR { // фон: глубина за срезом (детализация ГП), иначе небо/пещера
            let mut c = if y > w.top(x, z) || !w.loaded(x, z) { mix(0xb8d4f0, 0x3a6fc4, (y as f32 / 90.).clamp(0., 1.)) } else { 0x18161a };
            for k in 1..=q.depth2d { let b = w.get(x, y, z + k); if b != AIR { if b != VOID && kind(b) != K::Gas { c = mul(MATS[b as usize].col, 0.5 - k as f32 * 0.03) } break } }
            c
        } else if m == VOID { 0x101014 } else {
            let mut c = MATS[m as usize].col;
            if q.tex { c = tex(c, x, y, z) }
            if !glow(m) && q.shadows { let dd = w.top(x, z) - y; if dd > 0 { c = mul(c, (1. - dd as f32 * 0.07).max(0.35)) } }
            if q.tex && ppv >= 3. && ((fx - x as f32) * ppv < 1. || (fy - y as f32) * ppv < 1.) { c = mul(c, 0.82) }
            c
        };
        (c, 0.)
    });
}
fn render_text(cv: &mut Canvas, w: &World, v: &View, q: &Q, me: bool) {
    let s = q.txt;
    let (cw, chh) = (6 * s, 11 * s);
    let (cols, rows) = (cv.w / cw, (cv.h - TOP - 16) / chh);
    let (px, pz) = (v.p[0].floor() as i32, v.p[2].floor() as i32);
    for r in 0..rows { for c in 0..cols {
        let (x, z) = (px + c - cols / 2, pz + r - rows / 2);
        if !w.loaded(x, z) { cv.ch(c * cw, TOP + r * chh, '?', 0x303438, s); continue }
        let t = w.top(x, z);
        let a = w.get(x, t + 1, z);
        let m = if matches!(kind(a), K::Fire | K::Gas) { a } else { w.get(x, t, z) };
        let mut col = MATS[m as usize].col;
        if q.mono { col = if m == LIFE { 0xd0ffd0 } else { mul(0x33ff66, 0.15 + t as f32 / H as f32 * 0.4) } } else if q.tex { col = mul(col, 0.55 + t as f32 / H as f32 * 0.8) }
        cv.ch(c * cw, TOP + r * chh, MATS[m as usize].sym, col, s);
    } }
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

// ─── GAME: состояние, меню, ввод, игрок-бог, инструменты, бюджет кадра и телеметрия ─────────────
#[derive(Clone, Copy, PartialEq)]
enum Scr { Menu, Settings, Play }
struct Part { p: V3, v: V3, c: u32, t: f32 }
pub struct Game {
    scr: Scr, started: bool, ru: bool, pub quit: bool, help: bool, editor: bool, line: String, out: Vec<String>,
    sl: [i32; 6], sel: usize, q: Q, menu_sel: usize, drag: Option<usize>,
    sim: Sim, wgen: Gen, regen: u64, v: View, vel: V3, me: bool, ground: bool, tool: usize, mat: u8, brush: i32, paused: bool,
    pub cv: Canvas, sw: i32, sh: i32, wb: Vec<u32>, zb: Vec<f32>, ww: usize, wh: usize, parts: Vec<Part>,
    log: VecDeque<(String, String, f32)>, hover: Option<Hit>, mouse: (i32, i32),
    ms: [f32; 6], frame: f32, fps: f32, budget: f32, last: Instant, acc: f32, acc_life: f32, lag: f32, bott: Option<usize>, glitch: f32,
    spiral: Vec<(i32, i32)>, rng: u64, time: f32, cd: f32, pending: usize, placed: bool,
}
fn ms(t: Instant) -> f32 { t.elapsed().as_secs_f32() * 1000. }
impl Game {
    pub fn new(sw: i32, sh: i32) -> Game {
        let s = (sh / 540).max(1);
        let sl = [50, 55, 40, 40, 25, 80];
        let mut spiral: Vec<(i32, i32)> = (-7..=7).flat_map(|z| (-7..=7).map(move |x| (x, z))).collect();
        spiral.sort_by_key(|&(x, z)| x * x + z * z);
        Game {
            scr: Scr::Menu, started: false, ru: true, quit: false, help: false, editor: false, line: String::new(), out: vec![],
            sl, sel: 0, q: quality(&sl), menu_sel: 0, drag: None,
            sim: Sim::new(), wgen: parse_prompt(DEFAULT_PROMPT, 1).0, regen: 1, v: View { p: [8., 62., 8.], yaw: 0.6, pitch: -0.45 }, vel: [0.; 3],
            me: false, ground: false, tool: 0, mat: STONE, brush: 3, paused: false,
            cv: Canvas::new(sw / s, sh / s), sw, sh, wb: vec![], zb: vec![], ww: 0, wh: 0, parts: vec![],
            log: VecDeque::new(), hover: None, mouse: (0, 0),
            ms: [0.; 6], frame: 0., fps: 60., budget: 16.7, last: Instant::now(), acc: 0., acc_life: 0., lag: 0., bott: None, glitch: 0.,
            spiral, rng: 0x9E37_79B9_7F4A_7C15, time: 0., cd: 0., pending: 0, placed: false,
        }
    }
    fn l(&self, ru: &'static str, en: &'static str) -> &'static str { if self.ru { ru } else { en } }
    fn say(&mut self, ru: String, en: String) { self.log.push_back((ru, en, self.time)); while self.log.len() > self.q.hist { self.log.pop_front(); } }
    fn rf(&mut self) -> f32 { self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17; (self.rng >> 40) as f32 / 16_777_216. }
    pub fn wants_look(&self) -> bool { self.scr == Scr::Play && self.q.mode == 2 && !self.editor }
    fn resize(&mut self, w: i32, h: i32) { if w <= 0 || h <= 0 { return } (self.sw, self.sh) = (w, h); let s = (h / 540).max(1); self.cv = Canvas::new(w / s, h / s) }

    /// Один кадр: ввод → накопитель (чанки) → ОЗУ (жизнь) → ЦП (физика) → ГП (рендер) → Мир (сборка кадра).
    pub fn frame(&mut self, inp: &mut Input) {
        let now = Instant::now();
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
            }
            if self.acc > step * 4. { self.acc = 0.; self.lag = 1. }
        }
        self.lag = (self.lag - dt).max(0.);
        for (r, e) in std::mem::take(&mut self.sim.ev) { self.say(r, e) }
        for (p, c) in std::mem::take(&mut self.sim.fx) {
            if self.parts.len() >= self.q.parts { break }
            let v = [(self.rf() - 0.5) * 14., self.rf() * 10. + 3., (self.rf() - 0.5) * 14.];
            let t = 1. + self.rf() * 1.5;
            self.parts.push(Part { p, v, c, t })
        }
        let t = Instant::now();
        self.update_parts(dt);
        self.render_world();
        let gpu = ms(t);
        let t = Instant::now();
        self.compose();
        let ui = ms(t);
        for (i, x) in [ram, cpu, gpu, ssd, 0., ui].into_iter().enumerate() { if i != 4 { self.ms[i] += (x - self.ms[i]) * 0.1 } }
    }
    /// Конец кадра: учёт вывода, поиск узкого места, сон до бюджета целевого FPS.
    pub fn end_frame(&mut self, present_ms: f32, sleep: bool) {
        self.ms[5] += present_ms * 0.1;
        let work = ms(self.last);
        self.frame += (work - self.frame) * 0.1;
        self.ms[4] = self.frame;
        self.glitch = (self.frame / self.budget - 1.).clamp(0., 1.);
        self.bott = if self.frame > self.budget * 1.05 { [0usize, 1, 2, 3, 5].into_iter().copied().max_by(|&a, &b| self.ms[a].total_cmp(&self.ms[b]))
 } else { None };
        if sleep && work < self.budget { std::thread::sleep(Duration::from_secs_f32((self.budget - work) / 1000.)) }
    }
    fn stream(&mut self) {
        let (pcx, pcz) = ((self.v.p[0].floor() as i32) >> 4, (self.v.p[2].floor() as i32) >> 4);
        let rad = if self.q.mode == 1 { 7 } else { ((self.q.radius.max(self.q.view as i32) >> 4) + 1).clamp(2, 7) };
        let (mut n, mut pend) = (0, 0);
        for k in 0..self.spiral.len() {
            let (dx, dz) = self.spiral[k];
            let (cx, cz) = (pcx + dx, pcz + dz);
            if dx.abs() > rad || dz.abs() > rad || self.sim.w.slot[si(cx, cz)] == (cx, cz) { continue }
            if n < self.q.chunks { gen_chunk(&mut self.sim.w, &self.wgen, &self.q, cx, cz); n += 1 } else { pend += 1 }
        }
        self.pending = pend;
    }
    /// Ставит камеру над сушей с самой плотной клеточной жизнью рядом (после генерации нового мира).
    fn place(&mut self) {
        let w = &self.sim.w;
        let (px, pz) = (self.v.p[0].floor() as i32, self.v.p[2].floor() as i32);
        let mut best = (-1, px, pz);
        for cz in (-56..=56).step_by(4) { for cx in (-56..=56).step_by(4) {
            let (x, z) = (px + cx, pz + cz);
            if !w.loaded(x, z) || w.top(x, z) <= self.wgen.sea + 1 { continue }
            let mut s = 0;
            for dz in -6..=6 { for dx in -6..=6 { if w.get(x + dx, w.top(x + dx, z + dz), z + dz) == LIFE { s += 1 } } }
            if s > best.0 { best = (s, x, z) }
        } }
        self.v.p = [best.1 as f32 + 0.5, (w.top(best.1, best.2) + 16).min(H as i32 + 8) as f32, best.2 as f32 + 0.5];
        self.placed = true;
    }
    fn update_parts(&mut self, dt: f32) {
        let w = &self.sim.w;
        for p in &mut self.parts {
            p.v[1] -= 22. * dt;
            for i in 0..3 { p.p[i] += p.v[i] * dt }
            p.t -= dt;
            if matches!(kind(w.get(p.p[0].floor() as i32, p.p[1].floor() as i32, p.p[2].floor() as i32)), K::Solid | K::Powder) { p.v = [0.; 3]; p.t = p.t.min(0.4) }
        }
        self.parts.retain(|p| p.t > 0.);
    }

    // ── ввод ──
    fn input(&mut self, inp: &mut Input, dt: f32) {
        let (mx, my) = self.mouse;
        match self.scr {
            Scr::Menu => {
                if inp.hit(K_UP) { self.menu_sel = (self.menu_sel + 3) % 4 }
                if inp.hit(K_DOWN) { self.menu_sel = (self.menu_sel + 1) % 4 }
                let mut act = inp.hit(K_ENTER);
                if inp.click { if let Some(i) = self.menu_btns().iter().position(|b| inside(mx, my, *b)) { self.menu_sel = i; act = true } }
                if inp.hit(K_ESC) && self.started { self.scr = Scr::Play }
                if act {
                    match self.menu_sel {
                        0 => { self.scr = Scr::Play; if !self.started { self.started = true; self.say("Мир запущен. F1 - справка, Tab - редактор мира".into(), "World started. F1 - help, Tab - world editor".into()) } }
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
                if inp.lmb {
                    if self.drag.is_none() && inp.click { self.drag = (0..6).find(|&i| { let b = self.sl_bar(i); inside(mx, my, (b.0 - 4, b.1 - 6, b.2 + 8, b.3 + 12)) }) }
                    if let Some(i) = self.drag { let b = self.sl_bar(i); self.sel = i; self.sl[i] = ((mx - b.0) * 100 / b.2).clamp(1, 100) }
                } else { self.drag = None }
                if inp.hit(K_ESC) || inp.hit(K_ENTER) || (inp.click && inside(mx, my, self.back_btn())) { self.scr = Scr::Menu }
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
        self.sliders(inp);
        if inp.hit(K_UP) { self.brush = (self.brush + 1).min(12) }
        if inp.hit(K_DOWN) { self.brush = (self.brush - 1).max(1) }
        if inp.wheel != 0 { self.tool = (self.tool as i32 - inp.wheel).rem_euclid(5) as usize }
        if inp.hit(b'E') { self.mat = if self.mat >= LIFE { STONE } else { self.mat + 1 } }
        if inp.hit(b'Q') { self.mat = if self.mat <= STONE { LIFE } else { self.mat - 1 } }
        if inp.hit(b'P') { self.paused = !self.paused }
        if inp.hit(b'F') { self.toggle_me() }
        if self.q.mode == 2 { self.v.yaw += inp.look.0 as f32 * 0.003; self.v.pitch = (self.v.pitch - inp.look.1 as f32 * 0.003).clamp(-1.5, 1.5) }
        self.movement(inp, dt);
        let (mx, my) = self.mouse;
        self.hover = if my > TOP && my < self.cv.h - 16 { self.pick(mx, my) } else { None };
        self.cd -= dt;
        if inp.lmb && !inp.rmb && self.cd <= 0. { if let Some(h) = self.hover { self.apply_tool(h, inp.click) } }
    }
    fn solid_at(&self, f: V3) -> bool {
        for y in f[1].floor() as i32..=(f[1] + 1.75).floor() as i32 { for z in (f[2] - 0.3).floor() as i32..=(f[2] + 0.3).floor() as i32 { for x in (f[0] - 0.3).floor() as i32..=(f[0] + 0.3).floor() as i32 {
            if matches!(kind(self.sim.w.get(x, y, z)), K::Solid | K::Powder) { return true }
        } } }
        false
    }
    /// Бог летает свободно; воплощённый человек ходит с гравитацией, прыжком и автоподъёмом на 1 блок.
    fn movement(&mut self, inp: &Input, dt: f32) {
        let k = |c: u8| inp.held(c) as i32 as f32;
        let (fw, st, up) = (k(b'W') - k(b'S'), k(b'D') - k(b'A'), k(K_SPACE) - k(b'C'));
        let (mode, cy, sy) = (self.q.mode, self.v.yaw.cos(), self.v.yaw.sin());
        if !self.me {
            let sp = if inp.held(K_SHIFT) { 60. } else { 18. } * dt;
            let d = match mode { 2 => { let (f, r, _) = basis(self.v.yaw, self.v.pitch); add(add(sc(f, fw), sc(r, st)), [0., up, 0.]) } 1 => [st, fw, up * 0.25], _ => [st, up, -fw] };
            for i in 0..3 { self.v.p[i] += d[i] * sp }
            self.v.p[1] = self.v.p[1].clamp(1., H as f32 + 60.);
            return;
        }
        let wish = match mode { 2 => [cy * fw - sy * st, 0., sy * fw + cy * st], 1 => [st, 0., 0.], _ => [st, 0., -fw] };
        let spd = if inp.held(K_SHIFT) { 9. } else { 5. };
        let mut feet = [self.v.p[0], self.v.p[1] - 1.6, self.v.p[2]];
        let wet = kind(self.sim.w.get(feet[0].floor() as i32, (feet[1] + 0.5).floor() as i32, feet[2].floor() as i32)) == K::Liquid;
        (self.vel[0], self.vel[2]) = (wish[0] * spd, wish[2] * spd);
        self.vel[1] -= 25. * dt;
        if wet { self.vel[1] = self.vel[1].max(-3.); if inp.held(K_SPACE) { self.vel[1] = 4. } } // плавание
        if inp.held(K_SPACE) && self.ground { self.vel[1] = 8.5 }
        let was = self.ground;
        self.ground = false;
        for a in [0, 2, 1] {
            let mut n = feet;
            n[a] += self.vel[a] * dt;
            if !self.solid_at(n) { feet = n } else if a != 1 { n[1] += 1.05; if was && !self.solid_at(n) { feet = n } } else { if self.vel[1] < 0. { self.ground = true } self.vel[1] = 0. }
        }
        if mode == 1 { feet[2] = feet[2].floor() + 0.5 }
        if feet[1] < -5. { feet[1] = self.sim.w.top(feet[0] as i32, feet[2] as i32) as f32 + 2. }
        self.v.p = [feet[0], feet[1] + 1.6, feet[2]];
    }
    fn toggle_me(&mut self) {
        self.me = !self.me;
        self.vel = [0.; 3];
        if self.me {
            let (x, z) = match self.hover { Some(h) if self.q.mode != 0 => (h.prev[0], h.prev[2]), _ => (self.v.p[0].floor() as i32, self.v.p[2].floor() as i32) };
            self.v.p = [x as f32 + 0.5, self.sim.w.top(x, z) as f32 + 2.61, z as f32 + 0.5];
            self.say("Вы воплотились в человека: WASD - идти, Space - прыжок, F - выйти".into(), "You became a human: WASD walk, Space jump, F to leave".into());
        } else { self.say("Вы снова бог-наблюдатель".into(), "You are the watching god again".into()) }
    }
    /// Что под курсором: 3D — луч из пикселя; 2D — клетка среза; текст — столбец сверху.
    fn pick(&self, mx: i32, my: i32) -> Option<Hit> {
        let w = &self.sim.w;
        match self.q.mode {
            2 => {
                let (f, r, u) = basis(self.v.yaw, self.v.pitch);
                let asp = self.cv.w as f32 / self.cv.h as f32;
                let (sx, sy) = ((2. * mx as f32 / self.cv.w as f32 - 1.) * FOV * asp, (1. - 2. * my as f32 / self.cv.h as f32) * FOV);
                dda(w, self.v.p, nrm(add(f, add(sc(r, sx), sc(u, sy)))), 160., AIR).filter(|h| h.m != VOID)
            }
            1 => {
                let p = [(self.v.p[0] + (mx - self.cv.w / 2) as f32 / 4.).floor() as i32, (self.v.p[1] + (self.cv.h / 2 - my) as f32 / 4.).floor() as i32, self.v.p[2].floor() as i32];
                Some(Hit { p, prev: p, m: w.get(p[0], p[1], p[2]), t: 0., ax: 1, s: -1 })
            }
            _ => {
                let s = self.q.txt;
                let (cols, rows) = (self.cv.w / (6 * s), (self.cv.h - TOP - 16) / (11 * s));
                let (x, z) = (self.v.p[0].floor() as i32 + mx / (6 * s) - cols / 2, self.v.p[2].floor() as i32 + (my - TOP) / (11 * s) - rows / 2);
                let t = w.top(x, z);
                w.loaded(x, z).then_some(Hit { p: [x, t, z], prev: [x, t + 1, z], m: w.get(x, t, z), t: 0., ax: 1, s: -1 })
            }
        }
    }
    fn apply_tool(&mut self, h: Hit, click: bool) {
        let (b, lvl) = (self.brush, self.q.level);
        self.cd = 0.05;
        match self.tool {
            0 => if click { self.sim.explode(h.p, b + 1, lvl); self.say(format!("Взрыв радиусом {}", b + 1), format!("Blast, radius {}", b + 1)) },
            1 => { let m = self.mat; self.sphere(h.prev, b - 1, m, 1., |o| dens(o) < 20) }
            2 => self.sphere(h.p, b, AIR, 1., |o| o != BEDROCK && o != VOID),
            3 => self.sphere(h.prev, b, FIRE, 0.35, |o| o == AIR),
            _ => {
                let r = b * 2;
                for dz in -r..=r { for dx in -r..=r {
                    if dx * dx + dz * dz > r * r || !self.sim.chance(0.3) { continue }
                    let (x, z) = (h.p[0] + dx, h.p[2] + dz);
                    let t = self.sim.w.top(x, z);
                    if matches!(kind(self.sim.w.get(x, t, z)), K::Solid | K::Powder) { self.sim.put(x, t + 1, z, LIFE) }
                } }
            }
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
                    if self.q.level < 2 { self.out.push(self.l("Правила работают с уровня 2: поднимите ползунок ЦП выше 40%", "Rules run from level 2: raise the CPU slider above 40%").into()) }
                }
                None => self.out.push(self.l("Не понял правило. Формат: если <A> [рядом <B>] то <C> <N>%", "Can't parse. Format: if <A> [near <B>] then <C> <N>%").into()),
            },
            "правила" | "rules" => { for (i, r) in self.sim.rules.iter().enumerate() { self.out.push(format!("#{} {}", i + 1, rule_txt(r, ru))) } }
            "удалить" | "del" | "delete" => match ws.get(1).and_then(|s| s.parse::<usize>().ok()).filter(|&n| n >= 1 && n <= self.sim.rules.len()) {
                Some(n) => { self.sim.rules.remove(n - 1); self.sim.reindex(); self.out.push(self.l("Удалено.", "Removed.").into()) }
                None => self.out.push(self.l("Нет такого номера (см. «правила»)", "No such number (see 'rules')").into()),
            },
            "сброс" | "reset" => { self.sim.default_rules(); self.out.push(self.l("Правила по умолчанию восстановлены.", "Default rules restored.").into()) }
            "очистить" | "clear" => { self.sim.rules.clear(); self.sim.reindex(); self.out.push(self.l("Все правила удалены - химия выключена.", "All rules removed - chemistry is off.").into()) }
            "помощь" | "help" => {
                let lines = if ru { ["Мир: опишите словами - остров, горы, лес, пустыня, лёд, вулкан, пещеры, город, руины, жизнь, люди",
                    "Правило: если <A> [рядом <B>] то <C> <N>%  (A становится C, шанс N% за тик)",
                    "Команды: правила | удалить N | сброс | очистить | помощь"] }
                else { ["World: describe it - island, mountains, forest, desert, ice, volcano, caves, city, ruins, life, people",
                    "Rule: if <A> [near <B>] then <C> <N>%  (A becomes C, N% per tick)",
                    "Commands: rules | del N | reset | clear | help"] };
                self.out.extend(lines.iter().map(|s| s.to_string()));
                let names: Vec<&str> = MATS.iter().skip(1).map(|m| if ru { m.ru } else { m.en }).collect();
                for c in names.chunks(10) { self.out.push(format!("{}: {}", self.l("Вещества", "Materials"), c.join(", "))) }
            }
            _ => {
                let l = norm(line);
                let seed = hash(self.regen as i32, l.len() as i32, l.chars().map(|c| c as i32).sum(), 0x5EED);
                self.regen += 1;
                let (g, f) = parse_prompt(&l, seed);
                self.wgen = g;
                self.sim.w.reset();
                self.sim.bodies.clear();
                self.sim.claimed.clear();
                self.sim.supq.clear();
                self.parts.clear();
                self.me = false;
                self.placed = false;
                let names: Vec<&str> = f.iter().map(|&i| if ru { FEATS[i].1 } else { FEATS[i].2 }).collect();
                self.out.push(if f.is_empty() { self.l("Понятий не найдено - создаю случайный мир.", "No known concepts - generating a random world.").to_string() }
                    else if ru { format!("Создаю мир: {}", names.join(", ")) } else { format!("Generating: {}", names.join(", ")) });
                if f.contains(&1) { self.out.push(self.l("Люди и психология - уровень 4 (ЦП > 90%), появятся в следующих итерациях. Пока созданы поселения.", "People & psychology are level 4 (CPU > 90%), coming in next iterations. Settlements were created.").into()) }
                if f.contains(&8) { self.out.push(self.l("Клеточная жизнь (Конвей) идёт на любом уровне: зелёные клетки на поверхности.", "Cellular life (Conway) runs at any level: green cells on the surface.").into()) }
            }
        }
        let n = self.out.len();
        if n > 300 { self.out.drain(..n - 300); }
    }

    // ── вывод ──
    fn render_world(&mut self) {
        let q = self.q;
        if q.mode == 0 { self.cv.px.fill(0x05080a); render_text(&mut self.cv, &self.sim.w, &self.v, &q, self.me); return }
        let (ww, wh) = (((self.cv.w as f32 * q.res) as usize).max(32), ((self.cv.h as f32 * q.res) as usize).max(18));
        if (ww, wh) != (self.ww, self.wh) { (self.ww, self.wh, self.wb, self.zb) = (ww, wh, vec![0; ww * wh], vec![0.; ww * wh]) }
        let ppv = 4. * ww as f32 / self.cv.w as f32;
        if q.mode == 2 { render3d(&self.sim.w, &self.v, &q, &mut self.wb, &mut self.zb, ww, wh) } else { render2d(&self.sim.w, &self.v, &q, &mut self.wb, &mut self.zb, ww, wh, ppv) }
        let (f, r, u) = basis(self.v.yaw, self.v.pitch);
        let asp = ww as f32 / wh as f32;
        for p in &self.parts { // частицы обломков с проверкой глубины
            let rel = sub(p.p, self.v.p);
            let (px, py, s, d) = if q.mode == 2 {
                let z = dot(rel, f);
                if z < 0.3 { continue }
                (((dot(rel, r) / (z * FOV * asp) + 1.) * 0.5 * ww as f32) as i32, ((1. - dot(rel, u) / (z * FOV)) * 0.5 * wh as f32) as i32, ((0.08 * wh as f32 / (z * FOV)) as i32).clamp(1, 10), dot(rel, rel).sqrt())
            } else {
                if rel[2].abs() > 4. { continue }
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
            Scr::Play => { self.draw_hud(); if self.editor { self.draw_editor() } if self.help { self.draw_help() } }
        }
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
        let (lv, md, mem) = (LEVELS[q.level as usize][lg], MODES[q.mode as usize][lg], self.sim.w.mem_bytes as f32 / 1_048_576.);
        let s = if ru { format!("ЦЕЛЬ {:.0} FPS  РЕАЛ {:.1}  КАДР {:.1}/{:.1} мс  РАДИУС {}  УР.{} {}  ВИД {}  ТИК {:.0} Гц  ПАМЯТЬ {:.1}/{:.1} МБ  ЧАНКИ +{}", q.fps, self.fps, self.frame, self.budget, q.radius, q.level, lv, md, q.tick_hz, mem, q.mem_mb, self.pending) }
            else { format!("TARGET {:.0} FPS  REAL {:.1}  FRAME {:.1}/{:.1} ms  RADIUS {}  LVL {} {}  VIEW {}  TICK {:.0} Hz  MEMORY {:.1}/{:.1} MB  CHUNKS +{}", q.fps, self.fps, self.frame, self.budget, q.radius, q.level, lv, md, q.tick_hz, mem, q.mem_mb, self.pending) };
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
        if q.level >= 3 { let s = self.l("Уровни 3-4 (организмы, общество) - в следующих итерациях; сейчас работает уровень 2", "Levels 3-4 (organisms, society) come in next iterations; level 2 is running"); self.cv.text(4, TOP + 16, s, ACC, 1); }
        if self.lag > 0. { let s = self.l("Симуляция не успевает: тики пропущены", "Simulation can't keep up: ticks skipped"); self.cv.text(4, TOP + 28, s, RED, 1); }
        let mut y = ch - 30;
        for (r, e, t) in self.log.iter().rev().take(6) {
            let age = self.time - t;
            if age > 12. { break }
            self.cv.rect(4, y - 1, tw(if ru { r } else { e }, 1) + 4, 12, PANEL, 150);
            self.cv.text(6, y, if ru { r } else { e }, mix(TXT, 0x505860, age / 12.), 1);
            y -= 13;
        }
        let mn = if ru { MATS[self.mat as usize].ru } else { MATS[self.mat as usize].en };
        let s = if ru { format!("[Колесо] {}  [Q/E] {}  [Вверх/Вниз] кисть {}  [F] {}  [Tab] редактор  [F1] помощь  [P] {}  [Esc] меню", TOOLS[self.tool][0], mn, self.brush, if self.me { "выйти из тела" } else { "вселиться" }, if self.paused { "продолжить" } else { "пауза" }) }
            else { format!("[Wheel] {}  [Q/E] {}  [Up/Down] brush {}  [F] {}  [Tab] editor  [F1] help  [P] {}  [Esc] menu", TOOLS[self.tool][1], mn, self.brush, if self.me { "leave body" } else { "incarnate" }, if self.paused { "resume" } else { "pause" }) };
        self.cv.rect(0, ch - 16, cw, 16, PANEL, 215);
        self.cv.text(4, ch - 13, &s, TXT, 1);
        if let Some(h) = self.hover {
            if q.mode == 1 { let (x, y) = (cw / 2 + ((h.p[0] as f32 - self.v.p[0]) * 4.).floor() as i32, ch / 2 - ((h.p[1] as f32 + 1. - self.v.p[1]) * 4.).floor() as i32); self.cv.frame(x, y, 4, 4, 0xffffff) }
            let n = if h.m == VOID { "?" } else if ru { MATS[h.m as usize].ru } else { MATS[h.m as usize].en };
            let s = format!("{} {} [{} {} {}]", TOOLS[self.tool][lg], n, h.p[0], h.p[1], h.p[2]);
            let (mx, my) = self.mouse;
            self.cv.rect(mx + 12, my + 10, tw(&s, 1) + 6, 13, PANEL, 200);
            self.cv.text(mx + 15, my + 12, &s, TXT, 1);
        }
        let (x, y) = (cw / 2, ch / 2);
        if self.me && q.mode == 2 { self.cv.rect(x - 6, y, 13, 1, 0xffffff, 220); self.cv.rect(x, y - 6, 1, 13, 0xffffff, 220) }
        if self.me && q.mode == 1 { self.cv.rect(x - 2, y - 1, 4, 4, 0xffd8b0, 255); self.cv.rect(x - 2, y + 3, 4, 4, 0x3060c0, 255) }
        if self.paused { let s = self.l("ПАУЗА", "PAUSED"); self.cv.text((cw - tw(s, 3)) / 2, TOP + 44, s, ACC, 3); }
    }
    fn menu_btns(&self) -> Vec<(i32, i32, i32, i32)> { (0..4).map(|i| ((self.cv.w - 280) / 2, self.cv.h / 2 - 40 + i * 38, 280, 28)).collect() }
    fn sl_bar(&self, i: usize) -> (i32, i32, i32, i32) { ((self.cv.w - 620) / 2 + 190, TOP + 56 + i as i32 * 58, 330, 12) }
    fn back_btn(&self) -> (i32, i32, i32, i32) { ((self.cv.w - 180) / 2, TOP + 56 + 6 * 58 + 2, 180, 24) }
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
        let s = self.l("песочница: от клеток Конвея до разрушаемого мира", "sandbox: from Conway's cells to a destructible world");
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
            (0, true) => format!("радиус активного мира {} вокс., память мира {:.1} МБ, журнал {} событий", q.radius, q.mem_mb, q.hist),
            (0, false) => format!("active radius {} vox, world memory {:.1} MB, log of {} events", q.radius, q.mem_mb, q.hist),
            (1, true) => format!("тик {:.0} Гц, уровень {} - {}, глубина обрушений {} вокс.", q.tick_hz, q.level, lv, q.bfs),
            (1, false) => format!("tick {:.0} Hz, level {} - {}, collapse depth {} vox", q.tick_hz, q.level, lv, q.bfs),
            (2, true) => format!("разрешение {:.0}%, дальность {:.0}, тени {}, текстуры {}, прозрачность {}, частицы {}", q.res * 100., q.view, on(q.shadows), on(q.tex), on(q.water), q.parts),
            (2, false) => format!("resolution {:.0}%, view {:.0}, shadows {}, textures {}, transparency {}, particles {}", q.res * 100., q.view, on(q.shadows), on(q.tex), on(q.water), q.parts),
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
        self.cv.text((cw - tw(t, 2)) / 2, TOP + 16, t, ACC, 2);
        for i in 0..6 {
            let (b, sel) = (self.sl_bar(i), i == self.sel);
            let c = if sel { ACC } else { TXT };
            self.cv.text(x0, b.1 + 1, &format!("{}. {}", i + 1, SL_NAME[i][lg]), c, 1);
            self.cv.rect(b.0, b.1, b.2, b.3, 0x2a3238, 255);
            self.cv.rect(b.0, b.1, b.2 * self.sl[i] / 100, b.3, if sel { ACC } else { 0x4a90c0 }, 255);
            if sel { self.cv.frame(b.0 - 2, b.1 - 2, b.2 + 4, b.3 + 4, ACC) }
            self.cv.text(b.0 + b.2 + 10, b.1 + 1, &format!("{}%", self.sl[i]), c, 1);
            let info = self.sl_info(i);
            self.cv.text(x0, b.1 + 20, &info, DIM, 1);
        }
        let back = self.l("Назад", "Back");
        self.button(self.back_btn(), back, false);
        let s = self.l("[1-6] выбор  [Влево/Вправо] значение (Shift = x10)  [L] язык  [Esc] назад", "[1-6] select  [Left/Right] value (Shift = x10)  [L] language  [Esc] back");
        self.cv.text((cw - tw(s, 1)) / 2, ch - 38, s, DIM, 1);
    }
    fn draw_editor(&mut self) {
        let (cw, ch) = (self.cv.w, self.cv.h);
        let (x, y, w, h) = (16, ch / 2 - 40, cw - 32, ch / 2 + 16);
        self.cv.rect(x, y, w, h, PANEL, 235);
        self.cv.frame(x, y, w, h, ACC);
        let t = self.l("РЕДАКТОР МИРА   [Enter] выполнить   [Tab/Esc] закрыть", "WORLD EDITOR   [Enter] run   [Tab/Esc] close");
        self.cv.text(x + 8, y + 6, t, ACC, 1);
        let e = self.l("Примеры: остров с вулканом и лесом | зарождение жизни в пустыне | если камень рядом лава то лава 5% | правила | помощь",
            "Examples: volcanic island with forest | genesis of life in a desert | if stone near lava then lava 5% | rules | help");
        self.cv.text(x + 8, y + 20, e, DIM, 1);
        let n = ((h - 58) / 12) as usize;
        let start = self.out.len().saturating_sub(n);
        for (i, s) in self.out[start..].iter().enumerate() { self.cv.text(x + 8, y + 36 + i as i32 * 12, s, if s.starts_with('>') { ACC } else { TXT }, 1); }
        let cur = if (self.time * 2.) as i32 % 2 == 0 { "_" } else { " " };
        self.cv.rect(x + 4, y + h - 18, w - 8, 14, 0x1e262e, 255);
        self.cv.text(x + 8, y + h - 16, &format!("> {}{}", self.line, cur), TXT, 1);
    }
    fn draw_help(&mut self) {
        let ru = ["УПРАВЛЕНИЕ", "WASD - движение, Space/C - вверх/вниз (2D: глубина среза), Shift - быстрее",
            "ПКМ (зажать) - обзор мышью в 3D;  ЛКМ - применить инструмент под курсором",
            "Колесо - инструмент: Взрыв / Материал / Ластик / Огонь / Жизнь;  Q/E - материал",
            "Стрелки вверх/вниз - размер кисти;  F - вселиться в человека / выйти из тела",
            "1-6 - выбор ползунка, стрелки влево/вправо - значение (Shift = x10)",
            "Tab - редактор мира и правил;  P - пауза;  Esc - меню;  F1 - эта справка", "",
            "УРОВНИ (ползунок ЦП): 0 - клеточная жизнь, 1 - физика и обрушения, 2 - химия, огонь, правила;",
            "3 - организмы и 4 - общество появятся в следующих итерациях.",
            "Ползунок 6 (Мир): 1-33 текст, 34-66 2D-срез, 67-100 3D. Все режимы показывают один мир."];
        let en = ["CONTROLS", "WASD - move, Space/C - up/down (2D: slice depth), Shift - faster",
            "Hold RMB - mouse look in 3D;  LMB - apply tool under cursor",
            "Wheel - tool: Blast / Material / Eraser / Fire / Life;  Q/E - material",
            "Up/Down arrows - brush size;  F - incarnate as a human / leave the body",
            "1-6 - select slider, Left/Right arrows - value (Shift = x10)",
            "Tab - world & rule editor;  P - pause;  Esc - menu;  F1 - this help", "",
            "LEVELS (CPU slider): 0 - cellular life, 1 - physics & collapses, 2 - chemistry, fire, rules;",
            "3 - organisms and 4 - society arrive in next iterations.",
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
