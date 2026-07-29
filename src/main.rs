//! exoplanets — every known world outside the solar system, on one
//! diagram.
//!
//! How far out it orbits runs across, how big it is runs up, and our own
//! eight are drawn in white for scale. Press Enter and the whole system
//! unfolds with its habitable zone marked.

mod data;

use crust::style;
use crust::{seq, Crust, Cursor, Input, Pane, Popup};
use data::{catalog, mode_value, rgb_for, Method, Planet, MODES};
use std::collections::HashMap;
use std::io::Write;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// First row and column of the plot; the left margin holds size labels.
const PLOT_X: u16 = 7;
const PLOT_Y: u16 = 3;
/// Rows the readout under the diagram takes, status line included.
const DETAIL_H: u16 = 6;

/// The axes, in decades. 0.003 AU is a planet skimming its star, 316 AU
/// is out past anything the Voyagers will reach; 0.3 to 35 Earth radii
/// covers everything from the smallest rock to the largest puffball.
const X_MIN: f64 = -2.5;
const X_MAX: f64 = 2.5;
const Y_MIN: f64 = -0.55;
const Y_MAX: f64 = 1.55;

const RUST_RGB: (u8, u8, u8) = (247, 76, 0);
const ASK_RGB: (u8, u8, u8) = (255, 200, 120);
const ERR_RGB: (u8, u8, u8) = (255, 120, 100);
const KEY_RGB: (u8, u8, u8) = (120, 170, 220);
const VAL_RGB: (u8, u8, u8) = (235, 235, 240);
const ZONE_RGB: (u8, u8, u8) = (60, 130, 80);

struct App {
    sel: usize,
    /// Plot cell → the planets that land in it, biggest first.
    cells: HashMap<(u16, u16), Vec<usize>>,
    /// The plot size the cells were built for.
    dims: (u16, u16),
    mode: usize,
    chat: Vec<(String, String)>,
    status: Option<(String, (u8, u8, u8))>,
}

fn main() {
    let mut start: Option<String> = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-h" | "--help" => {
                println!("exoplanets — the known worlds, on one diagram (Fe2O3 suite)");
                println!();
                println!("Usage: exoplanets [PLANET|STAR]");
                println!();
                println!("  PLANET    start on this one, e.g. \"TRAPPIST-1 e\", 51peg, kepler-452b");
                println!();
                println!(
                    "{} planets from the NASA Exoplanet Archive: orbit, size, mass,",
                    catalog().all.len() - 8
                );
                println!("temperature, host star and distance. Enter opens the whole system");
                println!("with its habitable zone. Our own eight are drawn in for scale.");
                return;
            }
            "-v" | "--version" => {
                println!("exoplanets {VERSION}");
                return;
            }
            other => start = Some(other.to_string()),
        }
    }

    let cat = catalog();
    let sel = match start {
        Some(q) => match cat.find(&q) {
            Some(i) => i,
            None => {
                eprintln!("exoplanets: nothing called '{q}'");
                std::process::exit(1);
            }
        },
        None => cat.find("Earth").unwrap_or(0),
    };

    let mut app = App {
        sel,
        cells: HashMap::new(),
        dims: (0, 0),
        mode: 0,
        chat: Vec::new(),
        status: None,
    };

    Crust::init();
    Crust::set_app_identity("Exoplanets");
    Crust::clear_screen();
    let (mut cols, mut rows) = Crust::terminal_size();
    let mut footer = Pane::new(1, rows, cols, 1, 250, 236);
    footer.scroll = false;

    (cols, rows) = draw(&mut app, &mut footer);

    loop {
        let Some(key) = Input::getchr(None) else { continue };
        match key.as_str() {
            "q" | "Q" => break,
            "RIGHT" | "l" => app.step((1, 0)),
            "LEFT" | "h" => app.step((-1, 0)),
            "UP" | "k" => app.step((0, 1)),
            "DOWN" | "j" => app.step((0, -1)),
            "TAB" => app.cycle_cell(true),
            "S-TAB" => app.cycle_cell(false),
            "]" | "}" => app.step_system(1),
            "[" | "{" => app.step_system(-1),
            "1" | "2" | "3" | "4" | "5" | "6" => {
                app.mode = key.parse::<usize>().unwrap_or(1) - 1;
            }
            "m" => app.mode = (app.mode + 1) % MODES.len(),
            "M" => app.mode = (app.mode + MODES.len() - 1) % MODES.len(),
            "ENTER" => {
                show_system(&app, cols, rows);
                Crust::clear_screen();
            }
            "L" => {
                let here = app.cell_planets().to_vec();
                if here.len() < 2 {
                    app.say("this cell holds only this one", ERR_RGB);
                } else {
                    pick_list(&mut app, &here, &mut footer, cols, rows);
                    Crust::clear_screen();
                }
            }
            "/" => {
                let q = footer.ask_or_cancel("find: ", "");
                print!("{}", Cursor::hide_seq());
                std::io::stdout().flush().ok();
                if let Some(q) = q {
                    match catalog().find(&q) {
                        Some(i) => app.select(i),
                        None => app.say(&format!("nothing called {}", q.trim()), ERR_RGB),
                    }
                }
            }
            "c" => {
                let q = footer.ask_or_cancel("ask claude: ", "");
                print!("{}", Cursor::hide_seq());
                std::io::stdout().flush().ok();
                if let Some(q) = q {
                    if !q.trim().is_empty() {
                        footer.say(&style::rgb(" asking claude…", Some(ASK_RGB), None, ""));
                        std::io::stdout().flush().ok();
                        match ask_claude(&app, q.trim()) {
                            Ok(a) if !a.is_empty() => {
                                app.chat.push((q.trim().to_string(), a.clone()));
                                Crust::clear_screen();
                                let w = cols.saturating_sub(8).min(96);
                                let h = rows.saturating_sub(4).min(34);
                                let mut p = Popup::centered(w, h, 252, 234);
                                p.view(&format!(
                                    "{}\n\n{}\n\n{}",
                                    style::rgb(&app.cur().name, Some(ASK_RGB), None, "b"),
                                    style::dim(q.trim()),
                                    a
                                ));
                                Crust::clear_screen();
                            }
                            Ok(_) => app.say("claude returned nothing", ERR_RGB),
                            Err(e) => app.say(&format!("claude: {e}"), ERR_RGB),
                        }
                    }
                }
            }
            "e" => match export(&app) {
                Ok(p) => app.say(&format!("wrote {p}"), (140, 220, 140)),
                Err(e) => app.say(&format!("export: {e}"), ERR_RGB),
            },
            "r" | "C-L" => Crust::clear_screen(),
            "?" => {
                show_help(cols, rows);
                Crust::clear_screen();
            }
            "RESIZE" => Crust::clear_screen(),
            _ => {}
        }
        (cols, rows) = draw(&mut app, &mut footer);
    }

    Crust::cleanup();
    Crust::clear_screen();
}

// ─────────────────────────── the plot grid ───────────────────────────

/// Where a planet lands, as a fraction of the plot in each direction.
fn place(p: &Planet) -> (f64, f64) {
    let x = (p.smax.log10() - X_MIN) / (X_MAX - X_MIN);
    let y = (p.plot_radius().log10() - Y_MIN) / (Y_MAX - Y_MIN);
    (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
}

fn cell_of(p: &Planet, (w, h): (u16, u16)) -> (u16, u16) {
    let (x, y) = place(p);
    let cx = (x * (w.max(2) - 1) as f64).round() as u16;
    let cy = ((1.0 - y) * (h.max(2) - 1) as f64).round() as u16;
    (cx.min(w.saturating_sub(1)), cy.min(h.saturating_sub(1)))
}

impl App {
    fn cur(&self) -> &'static Planet {
        &catalog().all[self.sel]
    }

    fn say(&mut self, msg: &str, rgb: (u8, u8, u8)) {
        self.status = Some((msg.to_string(), rgb));
    }

    fn select(&mut self, i: usize) {
        if i != self.sel {
            self.chat.clear();
        }
        self.sel = i;
    }

    fn cur_cell(&self) -> (u16, u16) {
        cell_of(self.cur(), self.dims)
    }

    fn cell_planets(&self) -> &[usize] {
        static EMPTY: &[usize] = &[];
        self.cells.get(&self.cur_cell()).map(|v| v.as_slice()).unwrap_or(EMPTY)
    }

    fn build_cells(&mut self, dims: (u16, u16)) {
        self.dims = dims;
        self.cells.clear();
        for (i, p) in catalog().all.iter().enumerate() {
            self.cells.entry(cell_of(p, dims)).or_default().push(i);
        }
        // Our own worlds first, then biggest first: the landmark of a
        // cell is what you land on when you step into it.
        for list in self.cells.values_mut() {
            list.sort_by(|&a, &b| {
                let (x, y) = (&catalog().all[a], &catalog().all[b]);
                y.home
                    .cmp(&x.home)
                    .then(y.plot_radius().total_cmp(&x.plot_radius()))
            });
        }
    }

    /// One cell at a time: scan along the row or column for the next
    /// occupied cell, and if that line is empty take the nearest one on
    /// that side, so a step never dead-ends.
    fn step(&mut self, dir: (i32, i32)) {
        let (w, h) = self.dims;
        let (cc, cr) = self.cur_cell();
        let (mut c, mut r) = (cc as i32, cr as i32);
        loop {
            c += dir.0;
            r -= dir.1; // screen rows grow downward, size grows up
            if c < 0 || r < 0 || c >= w as i32 || r >= h as i32 {
                break;
            }
            if let Some(list) = self.cells.get(&(c as u16, r as u16)) {
                let target = list[0];
                self.select(target);
                return;
            }
        }
        let mut best: Option<(f64, usize)> = None;
        for (&(x, y), list) in self.cells.iter() {
            let (dx, dy) = (x as f64 - cc as f64, cr as f64 - y as f64);
            let along = dx * dir.0 as f64 + dy * dir.1 as f64;
            if along <= 0.0 {
                continue;
            }
            let across = (dx * dir.1 as f64).abs() + (dy * dir.0 as f64).abs();
            let cost = along + across * 2.0;
            if best.map_or(true, |(b, _)| cost < b) {
                best = Some((cost, list[0]));
            }
        }
        if let Some((_, i)) = best {
            self.select(i);
        }
    }

    /// Cycle through the planets sharing the selected planet's cell.
    fn cycle_cell(&mut self, forward: bool) {
        let list = self.cell_planets().to_vec();
        if list.len() < 2 {
            return;
        }
        let at = list.iter().position(|&i| i == self.sel).unwrap_or(0);
        let next = if forward {
            (at + 1) % list.len()
        } else {
            (at + list.len() - 1) % list.len()
        };
        self.select(list[next]);
    }

    /// In or out one planet, within the same system.
    fn step_system(&mut self, dir: i32) {
        let sys = catalog().system(&self.cur().host);
        if sys.len() < 2 {
            self.say(&format!("{} is the only one known there", self.cur().name), ERR_RGB);
            return;
        }
        let at = sys.iter().position(|&i| i == self.sel).unwrap_or(0) as i32;
        let next = (at + dir).rem_euclid(sys.len() as i32) as usize;
        self.select(sys[next]);
    }
}

// ─────────────────────────── drawing ─────────────────────────────────

fn draw(app: &mut App, footer: &mut Pane) -> (u16, u16) {
    // Ask the terminal how big it is every frame rather than trusting
    // the size we were told at startup: a window manager that resizes
    // after launch otherwise leaves half the screen holding whatever was
    // there before.
    let (cols, rows) = Crust::terminal_size();
    if cols != footer.w || rows != footer.y {
        footer.w = cols;
        footer.y = rows;
        footer.full_refresh();
        Crust::clear_screen();
    }
    let dims = plot_dims(cols, rows);
    if dims != app.dims {
        app.build_cells(dims);
        Crust::clear_screen();
    }
    draw_header(app, cols);
    draw_plot(app, cols, rows);
    draw_detail(app, cols, rows);
    footer.say(&style::dim(if cols < 108 {
        "←↓↑→ move · Tab cell · ⏎ system · m colour · / find · ? help · q"
    } else {
        "←↓↑→ move · Tab cell · [ ] system · ⏎ the system · 1-6/m colour · \
         L list · / find · c claude · e csv · ? help · q"
    }));
    print!("{}", Cursor::hide_seq());
    std::io::stdout().flush().ok();
    (cols, rows)
}

fn plot_dims(cols: u16, rows: u16) -> (u16, u16) {
    let w = cols.saturating_sub(PLOT_X + 1).max(2);
    let h = rows.saturating_sub(PLOT_Y + DETAIL_H + 2).max(2);
    (w, h)
}

fn draw_header(app: &App, cols: u16) {
    const BAR: (u8, u8, u8) = (38, 38, 38);
    let p = app.cur();
    let bg = style::set_bg_rgb(BAR.0, BAR.1, BAR.2);
    // Every style helper closes with a reset, which drops the bar's
    // background half way along the row. Re-assert it after each one.
    let armed = |s: &str| s.replace(style::RESET, &format!("{}{}", style::RESET, bg));

    let left = format!(
        " {}  {}  {}  {} ",
        style::rgb("exoplanets", Some(RUST_RGB), None, "b"),
        style::rgb(&p.name, Some((255, 220, 140)), None, "b"),
        style::dim(p.kind()),
        if p.home {
            style::rgb("· home", Some((160, 160, 170)), None, "")
        } else {
            style::dim(&format!("· {}", p.host))
        }
    );
    let right = format!(
        "{}  ·  {} planets ",
        MODES[app.mode],
        catalog().all.len() - 8
    );
    let pad = (cols as usize)
        .saturating_sub(crust::display_width(&left) + crust::display_width(&right));
    // On a narrow terminal the two halves meet; cut rather than wrap.
    let bar_line = format!(
        "{bg}{}{}{}",
        armed(&left),
        " ".repeat(pad.max(1)),
        armed(&style::dim(&right))
    );
    print!(
        "{}{}{}{}",
        Cursor::at(1, 1),
        crust::truncate_ansi(&bar_line, cols as usize),
        style::RESET,
        seq::ERASE_EOL
    );
}

/// The colour a cell takes: what the planets in it have in common.
fn cell_rgb(list: &[usize], mode: usize) -> (u8, u8, u8) {
    let all = &catalog().all;
    if list.iter().any(|&i| all[i].home) {
        return (255, 255, 255);
    }
    if mode == 0 {
        // The method most of them were found by. Averaging two distinct
        // colours would just make mud.
        let mut best = (0usize, Method::Other);
        for m in Method::ALL {
            let n = list.iter().filter(|&&i| all[i].method == m).count();
            if n > best.0 {
                best = (n, m);
            }
        }
        return best.1.rgb();
    }
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    for &i in list {
        let c = rgb_for(&all[i], mode);
        r += c.0 as u32;
        g += c.1 as u32;
        b += c.2 as u32;
    }
    let n = list.len().max(1) as u32;
    ((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

/// One planet is a dot, a crowd is a disc. Our own worlds keep their
/// initial, so the diagram has landmarks.
fn cell_glyph(list: &[usize]) -> String {
    let all = &catalog().all;
    if let Some(&i) = list.iter().find(|&&i| all[i].home) {
        return all[i].name.chars().next().unwrap_or('·').to_string();
    }
    match list.len() {
        0 | 1 => "·".to_string(),
        2..=4 => "•".to_string(),
        _ => "●".to_string(),
    }
}

fn draw_plot(app: &App, cols: u16, rows: u16) {
    let (w, h) = app.dims;
    let cur = app.cur_cell();
    let mut out = String::new();

    // Blank the row between the header and the plot.
    out.push_str(&Cursor::at(1, 2));
    out.push_str(seq::ERASE_EOL);

    for row in 0..h {
        out.push_str(&Cursor::at(1, PLOT_Y + row));
        // Size down the left edge, at the round numbers.
        let label = size_label(row, h);
        out.push_str(&style::dim(&format!("{label:>6}")));
        for col in 0..w {
            let list = app.cells.get(&(col, row));
            let here = (col, row) == cur;
            match list {
                Some(list) => {
                    let rgb = cell_rgb(list, app.mode);
                    let glyph = cell_glyph(list);
                    if here {
                        out.push_str(&style::rgb(&glyph, Some(ink_on(rgb)), Some(rgb), "b"));
                    } else {
                        out.push_str(&style::rgb(&glyph, Some(rgb), None, ""));
                    }
                }
                None if here => {
                    out.push_str(&style::rgb("+", Some((20, 20, 25)), Some((150, 150, 160)), "b"))
                }
                None => out.push(' '),
            }
        }
        out.push_str(style::RESET);
        // Erase the rest of the row rather than blanking it by hand: the
        // frame before may have reached further right.
        out.push_str(seq::ERASE_EOL);
    }

    // The orbit ruler along the bottom.
    let mut ruler = vec![' '; cols as usize];
    for (au, text) in [
        (0.01, "0.01"),
        (0.1, "0.1"),
        (1.0, "1 AU"),
        (10.0, "10"),
        (100.0, "100"),
    ] {
        let fx: f64 = (f64::log10(au) - X_MIN) / (X_MAX - X_MIN);
        let col = PLOT_X as usize + (fx * (w - 1) as f64).round() as usize;
        let start = col.saturating_sub(text.chars().count() / 2);
        for (k, ch) in text.chars().enumerate() {
            if start + k < ruler.len() {
                ruler[start + k] = ch;
            }
        }
    }
    let ruler: String = ruler.into_iter().collect();
    out.push_str(&Cursor::at(1, PLOT_Y + h));
    out.push_str(&style::dim(ruler.trim_end()));
    out.push_str(seq::ERASE_EOL);
    // And a word for each axis, tucked into the corners.
    out.push_str(&Cursor::at(1, PLOT_Y));
    out.push_str(&style::dim("R⊕"));
    let _ = rows;
    print!("{out}");
}

/// A size label for this plot row, on the rows where a round number
/// lands.
fn size_label(row: u16, h: u16) -> String {
    for (r, text) in [
        (0.5, "0.5"),
        (1.0, "1"),
        (2.0, "2"),
        (4.0, "4"),
        (10.0, "10"),
        (20.0, "20"),
    ] {
        let fy = (f64::log10(r) - Y_MIN) / (Y_MAX - Y_MIN);
        let want = ((1.0 - fy) * (h.max(2) - 1) as f64).round() as u16;
        if want == row {
            return text.to_string();
        }
    }
    String::new()
}

fn draw_detail(app: &App, cols: u16, rows: u16) {
    let p = app.cur();
    let y0 = rows.saturating_sub(DETAIL_H);
    let key = |k: &str| style::rgb(k, Some(KEY_RGB), None, "");
    let val = |v: &str| style::rgb(v, Some(VAL_RGB), None, "");
    let dash = || style::dim("—");

    let orbit = {
        let mut s = val(&format!("{} AU", fmt(p.smax)));
        if p.derived == 'a' {
            s.push_str(&style::dim("*"));
        }
        match p.period {
            Some(d) => {
                s.push_str(&style::dim(" · "));
                s.push_str(&val(&fmt_days(d)));
                if p.derived == 'p' {
                    s.push_str(&style::dim("*"));
                }
            }
            None => {}
        }
        if let Some(e) = p.ecc {
            if e > 0.0 {
                s.push_str(&style::dim(&format!("  ecc {e:.2}")));
            }
        }
        s
    };

    let size = {
        let mut s = match p.radius {
            Some(r) => val(&format!("{} R⊕", fmt(r))),
            None => format!("{} {}", dash(), style::dim("R⊕")),
        };
        s.push_str(&style::dim(" · "));
        s.push_str(&match p.mass {
            Some(m) => val(&format!("{} M⊕", fmt(m))),
            None => dash(),
        });
        if p.radius.is_none() {
            s.push_str(&style::dim(&format!(
                "   (drawn at {} R⊕, from the mass)",
                fmt(p.plot_radius())
            )));
        }
        s
    };

    let density = match (p.density(), p.gravity()) {
        (Some(d), Some(g)) => format!(
            "{}{}{}",
            val(&format!("{d:.2} g/cm³")),
            style::dim(" · "),
            val(&format!("{g:.2} g"))
        ),
        _ => dash(),
    };

    let climate = {
        let mut s = match p.eqt {
            Some(t) => val(&format!("{t:.0} K")),
            None => dash(),
        };
        if let Some(f) = p.insolation() {
            s.push_str(&style::dim(" · "));
            s.push_str(&val(&format!("{} × Earth's light", fmt(f))));
        }
        if p.in_hz() {
            s.push_str(&style::rgb("  in the zone", Some((130, 220, 140)), None, "b"));
        }
        s
    };

    let star = {
        let rgb = p.st_teff.map(data::star_rgb).unwrap_or(VAL_RGB);
        let mut bits: Vec<String> = Vec::new();
        if !p.spectype.is_empty() {
            bits.push(style::rgb(&p.spectype, Some(rgb), None, "b"));
        }
        if let Some(t) = p.st_teff {
            bits.push(style::rgb(&format!("{t:.0} K"), Some(rgb), None, ""));
        }
        if let Some(r) = p.st_rad {
            bits.push(val(&format!("{} R☉", fmt(r))));
        }
        if let Some(m) = p.st_mass {
            bits.push(val(&format!("{} M☉", fmt(m))));
        }
        if bits.is_empty() {
            dash()
        } else {
            bits.join(&style::dim(" · "))
        }
    };

    let found = if p.home {
        style::dim("known since before there were names for it")
    } else {
        format!(
            "{}{}",
            style::rgb(p.method.label(), Some(p.method.rgb()), None, ""),
            match p.year {
                Some(y) => style::dim(&format!(" · {y}")),
                None => String::new(),
            }
        )
    };

    let distance = match (p.dist_ly(), p.home) {
        (_, true) => style::dim("here"),
        (Some(d), _) => val(&format!("{} ly", fmt(d))),
        _ => dash(),
    };

    let system = {
        let n = catalog().system(&p.host).len();
        let here = app.cell_planets().len();
        format!(
            "{}{}",
            val(&format!("{} in {}", planets(n), p.host)),
            if here > 1 {
                style::dim(&format!("   ({here} planets in this cell, Tab)"))
            } else {
                String::new()
            }
        )
    };

    // Two columns. The left one is cut rather than allowed to run into
    // the right, which is what a long climate line wants to do.
    let col2 = 54usize.min(cols as usize / 2);
    let pair = |a: String, b: String| {
        let a = crust::truncate_ansi(&a, col2.saturating_sub(1));
        let w = crust::display_width(&a);
        format!("{a}{}{b}", " ".repeat(col2.saturating_sub(w)))
    };
    let lines = [
        pair(
            format!("{} {}", key("orbit   "), orbit),
            format!("{} {}", key("star     "), star),
        ),
        pair(
            format!("{} {}", key("size    "), size),
            format!("{} {}", key("found    "), found),
        ),
        pair(
            format!("{} {}", key("density "), density),
            format!("{} {}", key("distance "), distance),
        ),
        pair(
            format!("{} {}", key("climate "), climate),
            format!("{} {}", key("system   "), system),
        ),
        format!("{} {}", key("colour  "), legend(app.mode)),
    ];
    for (i, line) in lines.iter().enumerate() {
        print!(
            "{} {}{}{}",
            Cursor::at(1, y0 + i as u16),
            crust::truncate_ansi(line, cols.saturating_sub(2) as usize),
            style::RESET,
            seq::ERASE_EOL
        );
    }
    let status = match &app.status {
        Some((msg, rgb)) => style::rgb(&format!(" {msg}"), Some(*rgb), None, ""),
        None => String::new(),
    };
    print!(
        "{}{}{}{}",
        Cursor::at(1, y0 + lines.len() as u16),
        crust::truncate_ansi(&status, cols as usize),
        style::RESET,
        seq::ERASE_EOL
    );
}

/// What the colours mean under the current mode.
fn legend(mode: usize) -> String {
    let chip = |rgb: (u8, u8, u8), text: &str| {
        format!("{} {}", style::rgb("●", Some(rgb), None, ""), style::dim(text))
    };
    match mode {
        1 => bar("100 K", "2500 K"),
        2 => bar("1992", "today"),
        3 => bar("near", "far"),
        4 => format!(
            "{} {} {} {} {}",
            style::rgb("●", Some(data::star_rgb(3000.0)), None, ""),
            style::rgb("●", Some(data::star_rgb(4400.0)), None, ""),
            style::rgb("●", Some(data::star_rgb(5800.0)), None, ""),
            style::rgb("●", Some(data::star_rgb(9000.0)), None, ""),
            style::dim("cool red dwarf → hot white star")
        ),
        5 => bar("puffball", "iron"),
        _ => [
            chip(Method::Transit.rgb(), "transit"),
            chip(Method::RadialVelocity.rgb(), "wobble"),
            chip(Method::Microlensing.rgb(), "lensing"),
            chip(Method::Imaging.rgb(), "imaged"),
            chip(Method::Timing.rgb(), "timing"),
            chip(Method::Astrometry.rgb(), "astrometry"),
        ]
        .join(" "),
    }
}

fn bar(low: &str, high: &str) -> String {
    let mut s = String::new();
    for i in 0..14 {
        let rgb = data::heat(i as f64 / 13.0);
        s.push_str(&style::rgb("█", Some(rgb), None, ""));
    }
    format!("{} {s} {}", style::dim(low), style::dim(high))
}

/// Black or white, whichever the eye can read on this colour.
fn ink_on(bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let lum = 0.299 * bg.0 as f64 + 0.587 * bg.1 as f64 + 0.114 * bg.2 as f64;
    if lum > 140.0 {
        (10, 10, 15)
    } else {
        (255, 255, 255)
    }
}

/// Numbers a person would say out loud: three significant figures at
/// most, and no trailing zeroes.
fn fmt(v: f64) -> String {
    let a = v.abs();
    let s = if a >= 100.0 {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else if a >= 1.0 {
        format!("{v:.2}")
    } else if a >= 0.1 {
        format!("{v:.3}")
    } else if a >= 0.01 {
        format!("{v:.4}")
    } else {
        format!("{v:.5}")
    };
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn fmt_days(d: f64) -> String {
    if d < 2.0 {
        format!("{:.1} h", d * 24.0)
    } else if d < 800.0 {
        format!("{} d", fmt(d))
    } else {
        format!("{} yr", fmt(d / 365.25))
    }
}

fn planets(n: usize) -> String {
    if n == 1 {
        "1 planet".to_string()
    } else {
        format!("{n} planets")
    }
}

// ─────────────────────────── popups ──────────────────────────────────

/// The selected planet's whole system: every sibling on a log scale from
/// the star, with the habitable zone painted behind them.
fn show_system(app: &App, cols: u16, rows: u16) {
    let all = &catalog().all;
    let p = app.cur();
    let sys = catalog().system(&p.host);
    let w = cols.saturating_sub(8).min(88);

    let star_rgb = p.st_teff.map(data::star_rgb).unwrap_or((235, 235, 240));
    let mut head = vec![style::rgb(&p.host, Some(star_rgb), None, "b")];
    if !p.spectype.is_empty() {
        head.push(style::rgb(&p.spectype, Some(star_rgb), None, ""));
    }
    if let Some(t) = p.st_teff {
        head.push(style::dim(&format!("{t:.0} K")));
    }
    if let Some(l) = p.st_lum() {
        head.push(style::dim(&format!("{} L☉", fmt(l))));
    }
    if let Some(d) = p.dist_ly() {
        if !p.home {
            head.push(style::dim(&format!("{} ly", fmt(d))));
        }
    }
    let mut text = format!("{}\n{}\n\n", head.join(&style::dim(" · ")), style::dim(&planets(sys.len())));

    text.push_str(&orbit_strip(&sys, app.sel, w.saturating_sub(4)));
    text.push_str("\n\n");

    text.push_str(&style::dim(&format!(
        "  {:<14}{:>9}{:>11}{:>10}{:>10}{:>8}\n",
        "planet", "AU", "period", "size", "mass", "T eq"
    )));
    for &i in &sys {
        let q = &all[i];
        let mark = if i == app.sel { "▸" } else { " " };
        let name: String = q.name.chars().take(14).collect();
        let row = format!(
            "{mark} {:<14}{:>9}{:>11}{:>10}{:>10}{:>8}",
            name,
            fmt(q.smax),
            q.period.map(fmt_days).unwrap_or_else(|| "—".into()),
            q.radius.map(|r| format!("{} R⊕", fmt(r))).unwrap_or_else(|| "—".into()),
            q.mass.map(|m| format!("{} M⊕", fmt(m))).unwrap_or_else(|| "—".into()),
            q.eqt.map(|t| format!("{t:.0} K")).unwrap_or_else(|| "—".into()),
        );
        let rgb = if i == app.sel { (255, 220, 140) } else { (200, 200, 210) };
        text.push_str(&style::rgb(&row, Some(rgb), None, if i == app.sel { "b" } else { "" }));
        if q.in_hz() {
            text.push_str(&style::rgb("  ✓ zone", Some((130, 220, 140)), None, ""));
        }
        text.push('\n');
    }

    if let Some((a, b)) = p.hz() {
        text.push_str(&format!(
            "\n{}\n",
            style::dim(&format!(
                "The green band is where water could stay liquid on a rocky world: \
                 {}–{} AU for this star.",
                fmt(a),
                fmt(b)
            ))
        ));
    }
    text.push_str(&style::dim("\nESC or q closes this."));

    // Fit the box to what is in it: a three-planet system should not
    // open a thirty-row window.
    let h = (text.lines().count() as u16 + 1).min(rows.saturating_sub(4));
    let mut pop = Popup::centered(w, h, 252, 234);
    pop.view(&text);
}

/// The system drawn on a log axis: the star at the left, each planet a
/// letter, the habitable zone painted behind them.
fn orbit_strip(sys: &[usize], sel: usize, w: u16) -> String {
    let all = &catalog().all;
    let w = w.max(20) as usize;
    let lo = sys.iter().map(|&i| all[i].smax).fold(f64::MAX, f64::min) / 2.0;
    let hi = sys.iter().map(|&i| all[i].smax).fold(0.0, f64::max) * 2.0;
    let (l0, l1) = (lo.log10(), hi.log10());
    let at = |au: f64| {
        let f = (au.log10() - l0) / (l1 - l0).max(1e-9);
        ((f * (w - 1) as f64).round() as isize).clamp(0, w as isize - 1) as usize
    };

    // The zone first, as a background band, then the planets on top.
    let zone = all[sys[0]].hz();
    let mut band = vec![false; w];
    if let Some((a, b)) = zone {
        for (c, slot) in band.iter_mut().enumerate() {
            let au = 10f64.powf(l0 + c as f64 / (w - 1) as f64 * (l1 - l0));
            *slot = au >= a && au <= b;
        }
    }
    let mut glyph: Vec<Option<(char, bool)>> = vec![None; w];
    for &i in sys {
        let c = at(all[i].smax);
        // The last character of the name is the planet's letter.
        let ch = all[i].name.chars().last().unwrap_or('•');
        glyph[c] = Some((ch, i == sel));
    }

    let mut s = String::from("  ");
    s.push_str(&style::rgb("★", Some((255, 220, 120)), None, "b"));
    for c in 0..w {
        let bg = if band[c] { Some(ZONE_RGB) } else { None };
        match glyph[c] {
            Some((ch, true)) => s.push_str(&style::rgb(
                &ch.to_string(),
                Some((20, 20, 25)),
                Some((255, 220, 140)),
                "b",
            )),
            Some((ch, false)) => {
                s.push_str(&style::rgb(&ch.to_string(), Some((235, 235, 240)), bg, "b"))
            }
            None => s.push_str(&style::rgb("·", Some((90, 90, 100)), bg, "")),
        }
    }
    // The scale under it: the two ends and the middle.
    let mid = 10f64.powf((l0 + l1) / 2.0);
    let label = |au: f64| format!("{} AU", fmt(au));
    let (a, b, c) = (label(lo), label(mid), label(hi));
    let mut ruler = vec![' '; w + 3];
    for (start, text) in [
        (3usize, a),
        (3 + w / 2 - b.len() / 2, b),
        (3 + w.saturating_sub(c.len()), c),
    ] {
        for (k, ch) in text.chars().enumerate() {
            if start + k < ruler.len() {
                ruler[start + k] = ch;
            }
        }
    }
    format!("{s}\n{}", style::dim(&ruler.into_iter().collect::<String>()))
}

/// A list of planets to walk and pick from, captioned by whatever the
/// current colour mode is showing.
fn pick_list(app: &mut App, list: &[usize], footer: &mut Pane, cols: u16, rows: u16) {
    if list.len() < 2 {
        return;
    }
    let all = &catalog().all;
    let lines: Vec<String> = list
        .iter()
        .map(|&i| {
            let p = &all[i];
            let name: String = p.name.chars().take(22).collect();
            format!(
                " {} {:>9} AU {:>10} {:>22}",
                style::rgb(&format!("{name:<22}"), Some(rgb_for(p, app.mode)), None, ""),
                fmt(p.smax),
                p.radius.map(|r| format!("{} R⊕", fmt(r))).unwrap_or_else(|| "—".into()),
                style::dim(&mode_value(p, app.mode)),
            )
        })
        .collect();
    let w = 78.min(cols.saturating_sub(4));
    let h = (lines.len() as u16).min(rows.saturating_sub(6)).max(1);
    let mut pop = Popup::centered(w, h, 253, 236);
    pop.pane.index = list.iter().position(|&i| i == app.sel).unwrap_or(0);
    let picked = pop.modal(&lines.join("\n"));
    pop.dismiss(&mut [footer]);
    if let Some(ix) = picked {
        if let Some(&i) = list.get(ix) {
            app.select(i);
        }
    }
}

fn show_help(cols: u16, rows: u16) {
    let help = format!(
        "{}\n\n  \
         How far out a planet orbits runs across, how big it is runs up.\n  \
         {} of them, and our own eight in white for scale.\n\n  \
         MOVING\n    \
           ← →, h l          nearer / further out\n    \
           ↑ ↓, k j          smaller / bigger\n    \
           Tab / Shift-Tab   through the planets sharing one cell\n    \
           [ ]               in and out through one system\n    \
           L                 list what is in this cell, and pick\n    \
           /                 find one: \"TRAPPIST-1 e\", 51peg, kepler-452b\n\n  \
         LOOKING\n    \
           ENTER             the whole system, with its habitable zone\n    \
           1-6, m            colour: found · temperature · year · distance ·\n    \
                             host star · density\n    \
           c                 ask Claude about this planet\n    \
           e                 write the catalogue to ~/exoplanets.csv\n    \
           ? q               this help · quit\n\n  \
         The numbers are the NASA Exoplanet Archive's composite table, the\n  \
         one value per planet its curators settled on. A star marks an orbit\n  \
         worked out from Kepler's third law rather than measured.\n  \
         The habitable zone is where a rocky planet would get between 1.1 and\n  \
         0.53 times Earth's sunlight. It says where water could stay liquid,\n  \
         not that anything lives there.\n\n  \
         {}",
        style::rgb(&format!("exoplanets v{VERSION}"), Some(ASK_RGB), None, "b"),
        catalog().all.len() - 8,
        style::dim("ESC or q closes this.")
    );
    let w = cols.saturating_sub(8).min(80);
    let h = rows.saturating_sub(4).min(34);
    let mut p = Popup::centered(w, h, 252, 234);
    p.view(&help);
}

// ─────────────────────────── the rest ────────────────────────────────

fn csv(v: &str) -> String {
    if v.contains([',', '"', '\n']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

fn export(app: &App) -> Result<String, String> {
    let all = &catalog().all;
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let path = format!("{home}/exoplanets.csv");
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&all[a], &all[b]);
        match app.mode {
            1 => y.eqt.unwrap_or(0.0).total_cmp(&x.eqt.unwrap_or(0.0)),
            2 => x.year.unwrap_or(9999).cmp(&y.year.unwrap_or(9999)),
            3 => x.dist_pc.unwrap_or(f64::MAX).total_cmp(&y.dist_pc.unwrap_or(f64::MAX)),
            4 => y.st_teff.unwrap_or(0.0).total_cmp(&x.st_teff.unwrap_or(0.0)),
            5 => y.density().unwrap_or(0.0).total_cmp(&x.density().unwrap_or(0.0)),
            _ => x.host.cmp(&y.host).then(x.smax.total_cmp(&y.smax)),
        }
    });
    let mut out = String::with_capacity(all.len() * 150);
    out.push_str(
        "name,host,planets_in_system,method,year,smax_au,period_d,radius_earth,mass_earth,\
         density_g_cm3,gravity_g,eqt_k,insolation_earth,eccentricity,hz_inner_au,hz_outer_au,\
         in_hz,star_type,star_teff_k,star_radius_sun,star_mass_sun,distance_ly,derived\n",
    );
    let n = |v: Option<f64>| v.map(|x| format!("{x:.5}")).unwrap_or_default();
    for i in order {
        let p = &all[i];
        let (hz_a, hz_b) = match p.hz() {
            Some((a, b)) => (format!("{a:.5}"), format!("{b:.5}")),
            None => (String::new(), String::new()),
        };
        out.push_str(&format!(
            "{},{},{},{},{},{:.5},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv(&p.name),
            csv(&p.host),
            catalog().system(&p.host).len(),
            csv(p.method.label()),
            p.year.map(|y| y.to_string()).unwrap_or_default(),
            p.smax,
            n(p.period),
            n(p.radius),
            n(p.mass),
            n(p.density()),
            n(p.gravity()),
            n(p.eqt),
            n(p.insolation()),
            n(p.ecc),
            hz_a,
            hz_b,
            if p.in_hz() { "yes" } else { "no" },
            csv(&p.spectype),
            n(p.st_teff),
            n(p.st_rad),
            n(p.st_mass),
            n(p.dist_ly()),
            p.derived,
        ));
    }
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(path)
}

fn claude_run(prompt: &str, input: &str) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("claude")
        .args(["-p", prompt])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "claude not on PATH".to_string(),
            _ => format!("spawn: {e}"),
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(input.as_bytes()).map_err(|e| format!("stdin: {e}"))?;
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| format!("wait: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().next().unwrap_or("(no message)").chars().take(80).collect());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn ask_claude(app: &App, question: &str) -> Result<String, String> {
    let all = &catalog().all;
    let p = app.cur();
    let mut ctx = format!(
        "Planet: {} — {}, orbiting {} at {} AU",
        p.name,
        p.kind(),
        p.host,
        fmt(p.smax)
    );
    if let Some(d) = p.period {
        ctx.push_str(&format!(" every {}", fmt_days(d)));
    }
    ctx.push_str(".\n");
    ctx.push_str(&format!(
        "Radius {} Earth radii, mass {} Earth masses",
        p.radius.map(fmt).unwrap_or_else(|| "unknown".into()),
        p.mass.map(fmt).unwrap_or_else(|| "unknown".into()),
    ));
    if let Some(d) = p.density() {
        ctx.push_str(&format!(", density {d:.2} g/cm³"));
    }
    ctx.push_str(".\n");
    if let Some(t) = p.eqt {
        ctx.push_str(&format!("Equilibrium temperature {t:.0} K"));
        if let Some(f) = p.insolation() {
            ctx.push_str(&format!(", {} times Earth's sunlight", fmt(f)));
        }
        ctx.push_str(".\n");
    }
    if !p.home {
        ctx.push_str(&format!(
            "Found by {}{}.\n",
            p.method.label(),
            p.year.map(|y| format!(" in {y}")).unwrap_or_default()
        ));
    }
    ctx.push_str(&format!(
        "Host star: {} {}, {} K, {} solar radii, {} solar masses",
        p.host,
        p.spectype,
        p.st_teff.map(|t| format!("{t:.0}")).unwrap_or_else(|| "unknown".into()),
        p.st_rad.map(fmt).unwrap_or_else(|| "unknown".into()),
        p.st_mass.map(fmt).unwrap_or_else(|| "unknown".into()),
    ));
    if let Some(d) = p.dist_ly() {
        ctx.push_str(&format!(", {} light years away", fmt(d)));
    }
    ctx.push_str(".\n");
    if let Some((a, b)) = p.hz() {
        ctx.push_str(&format!(
            "Habitable zone {}–{} AU, so this planet is {}.\n",
            fmt(a),
            fmt(b),
            if p.in_hz() { "inside it" } else { "outside it" }
        ));
    }
    let sys = catalog().system(&p.host);
    if sys.len() > 1 {
        let names: Vec<String> = sys
            .iter()
            .map(|&i| format!("{} at {} AU", all[i].name, fmt(all[i].smax)))
            .collect();
        ctx.push_str(&format!("The system: {}.\n", names.join(", ")));
    }
    if !app.chat.is_empty() {
        ctx.push_str("\nEarlier in this conversation:\n");
        for (q, a) in &app.chat {
            ctx.push_str(&format!("User: {q}\nYou: {a}\n\n"));
        }
    }
    ctx.push_str(&format!("\nQuestion: {question}\n"));
    claude_run(
        "You are an astronomer answering inside a terminal app. Answer in plain text, no \
         markdown, under 200 words unless the question demands more. The numbers above come \
         from the NASA Exoplanet Archive.",
        &ctx,
    )
}
