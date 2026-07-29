//! The catalogue, and what can be worked out from it.

use std::sync::OnceLock;

/// NASA Exoplanet Archive, composite parameters, one row per planet.
const TABLE: &str = include_str!("../data/exoplanets.csv");

/// How a planet was found. The archive spells out a dozen methods; these
/// are the ones with enough planets to be worth a colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    Transit,
    RadialVelocity,
    Microlensing,
    Imaging,
    Timing,
    Astrometry,
    Other,
}

impl Method {
    pub fn parse(s: &str) -> Method {
        match s {
            "Transit" => Method::Transit,
            "Radial Velocity" => Method::RadialVelocity,
            "Microlensing" => Method::Microlensing,
            "Imaging" => Method::Imaging,
            "Astrometry" => Method::Astrometry,
            // Transit timing, eclipse timing, pulsar timing, pulsation
            // timing: all the same trick, a clock that runs early and
            // late because something is tugging on it.
            s if s.contains("Timing") => Method::Timing,
            _ => Method::Other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Method::Transit => "transit",
            Method::RadialVelocity => "radial velocity",
            Method::Microlensing => "microlensing",
            Method::Imaging => "direct imaging",
            Method::Timing => "timing",
            Method::Astrometry => "astrometry",
            Method::Other => "other",
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Method::Transit => (105, 175, 255),
            Method::RadialVelocity => (255, 175, 80),
            Method::Microlensing => (190, 130, 255),
            Method::Imaging => (120, 230, 150),
            Method::Timing => (255, 120, 170),
            Method::Astrometry => (110, 230, 225),
            Method::Other => (170, 170, 175),
        }
    }

    pub const ALL: [Method; 7] = [
        Method::Transit,
        Method::RadialVelocity,
        Method::Microlensing,
        Method::Imaging,
        Method::Timing,
        Method::Astrometry,
        Method::Other,
    ];
}

/// One planet. Earth units throughout: radii in Earth radii, masses in
/// Earth masses, so Jupiter is 11.2 and 318.
#[derive(Clone, Debug)]
pub struct Planet {
    pub name: String,
    pub host: String,
    pub method: Method,
    pub year: Option<u32>,
    /// Orbital period in days.
    pub period: Option<f64>,
    /// Semi-major axis in AU. Every planet here has one, measured or
    /// worked out from the period.
    pub smax: f64,
    pub radius: Option<f64>,
    pub mass: Option<f64>,
    /// Equilibrium temperature in kelvin.
    pub eqt: Option<f64>,
    pub ecc: Option<f64>,
    pub st_teff: Option<f64>,
    pub st_rad: Option<f64>,
    pub st_mass: Option<f64>,
    pub spectype: String,
    /// Distance to the system in parsecs.
    pub dist_pc: Option<f64>,
    /// 'a' or 'p' when that value came from Kepler's third law rather
    /// than from a measurement.
    pub derived: char,
    /// True for the eight worlds of our own system, which the archive
    /// does not carry and which are drawn for scale.
    pub home: bool,
}

impl Planet {
    /// Bulk density in g/cm³, if both mass and radius are known. Earth
    /// is 5.51, Saturn 0.69, and the puffiest hot Jupiters are under
    /// 0.1, which is cork.
    pub fn density(&self) -> Option<f64> {
        match (self.mass, self.radius) {
            (Some(m), Some(r)) if r > 0.0 => Some(5.513 * m / (r * r * r)),
            _ => None,
        }
    }

    /// Surface gravity relative to Earth's.
    pub fn gravity(&self) -> Option<f64> {
        match (self.mass, self.radius) {
            (Some(m), Some(r)) if r > 0.0 => Some(m / (r * r)),
            _ => None,
        }
    }

    /// What kind of world the size suggests. The boundaries are the
    /// conventional ones, and the radius valley at about 1.8 Earth radii
    /// is real: planets avoid it.
    pub fn kind(&self) -> &'static str {
        match self.radius {
            Some(r) if r < 1.25 => "Earth-sized",
            Some(r) if r < 2.0 => "super-Earth",
            Some(r) if r < 6.0 => "Neptune-like",
            Some(r) if r < 15.0 => "gas giant",
            Some(_) => "very large",
            None => match self.mass {
                Some(m) if m < 2.0 => "Earth-mass",
                Some(m) if m < 10.0 => "super-Earth",
                Some(m) if m < 50.0 => "Neptune-mass",
                Some(_) => "giant",
                None => "unknown",
            },
        }
    }

    pub fn dist_ly(&self) -> Option<f64> {
        self.dist_pc.map(|d| d * 3.261_563_8)
    }

    /// Where the planet sits on the size axis. Its radius, nearly
    /// always; for the few dozen that have only a mass, a radius worked
    /// out from that mass, and the readout says so.
    pub fn plot_radius(&self) -> f64 {
        self.radius
            .or_else(|| self.mass.map(mass_to_radius))
            .unwrap_or(1.0)
    }

    /// The star's luminosity in solar units, from its size and surface
    /// temperature: L = R²(T/T☉)⁴.
    pub fn st_lum(&self) -> Option<f64> {
        match (self.st_rad, self.st_teff) {
            (Some(r), Some(t)) if r > 0.0 && t > 0.0 => Some(r * r * (t / 5772.0).powi(4)),
            _ => None,
        }
    }

    /// The habitable zone in AU: where a planet gets between 1.1 and
    /// 0.53 times Earth's sunlight. Runaway greenhouse on the inside,
    /// maximum greenhouse on the outside, both scaled by the square root
    /// of the star's luminosity.
    pub fn hz(&self) -> Option<(f64, f64)> {
        self.st_lum().map(|l| ((l / 1.1).sqrt(), (l / 0.53).sqrt()))
    }

    /// Does this one sit in the zone? That says nothing about whether it
    /// is habitable; a Jupiter at 1 AU is in the zone too.
    pub fn in_hz(&self) -> bool {
        matches!(self.hz(), Some((a, b)) if self.smax >= a && self.smax <= b)
    }

    /// Sunlight relative to Earth's.
    pub fn insolation(&self) -> Option<f64> {
        self.st_lum().map(|l| l / (self.smax * self.smax))
    }
}

/// A radius from a mass, by the broken power law Chen and Kipping fitted
/// to everything that has both (Forecaster, 2017). Rock packs tighter
/// the more of it there is, gas puffs up, and past a Jupiter mass a
/// planet stops growing altogether: degeneracy holds the size flat.
pub fn mass_to_radius(m: f64) -> f64 {
    match m {
        m if m < 2.04 => 1.008 * m.powf(0.279),
        m if m < 132.0 => 0.808 * m.powf(0.589),
        _ => 17.74 * m.powf(-0.044),
    }
}

pub struct Catalog {
    pub all: Vec<Planet>,
}

impl Catalog {
    /// Every planet of one host, innermost first.
    pub fn system(&self, host: &str) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.all.len())
            .filter(|&i| self.all[i].host == host)
            .collect();
        v.sort_by(|&a, &b| self.all[a].smax.total_cmp(&self.all[b].smax));
        v
    }

    /// Find by planet name, then by host, case- and space-insensitively.
    pub fn find(&self, query: &str) -> Option<usize> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return None;
        }
        let squash = |s: &str| s.to_lowercase().replace([' ', '-', '_'], "");
        let qs = squash(&q);
        self.all
            .iter()
            .position(|p| p.name.to_lowercase() == q)
            .or_else(|| self.all.iter().position(|p| squash(&p.name) == qs))
            .or_else(|| self.all.iter().position(|p| squash(&p.host) == qs))
            .or_else(|| self.all.iter().position(|p| squash(&p.name).contains(&qs)))
            .or_else(|| self.all.iter().position(|p| squash(&p.host).contains(&qs)))
    }
}

/// Our own eight, for scale. Values are the standard ones; Earth is the
/// unit for radius and mass by definition.
const HOME: [(&str, f64, f64, f64, f64, f64); 8] = [
    // name, period (d), semi-major (AU), radius (R⊕), mass (M⊕), equilibrium T (K)
    ("Mercury", 87.97, 0.387, 0.383, 0.055, 440.0),
    ("Venus", 224.7, 0.723, 0.949, 0.815, 232.0),
    ("Earth", 365.26, 1.0, 1.0, 1.0, 255.0),
    ("Mars", 686.98, 1.524, 0.532, 0.107, 210.0),
    ("Jupiter", 4332.6, 5.204, 11.21, 317.8, 122.0),
    ("Saturn", 10759.0, 9.583, 9.45, 95.16, 90.0),
    ("Uranus", 30687.0, 19.19, 4.01, 14.54, 64.0),
    ("Neptune", 60190.0, 30.07, 3.88, 17.15, 51.0),
];

pub fn catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| {
        let mut all = Vec::with_capacity(6400);
        for line in TABLE.lines() {
            if line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 19 {
                continue;
            }
            let num = |s: &str| s.trim().parse::<f64>().ok();
            let Some(smax) = num(f[6]) else { continue };
            // A planet with neither a size nor a mass has nowhere to sit
            // on the diagram. Four of them; they stay out.
            if num(f[7]).is_none() && num(f[8]).is_none() {
                continue;
            }
            all.push(Planet {
                name: f[0].to_string(),
                host: f[1].to_string(),
                method: Method::parse(f[3]),
                year: f[4].parse().ok(),
                period: num(f[5]),
                smax,
                radius: num(f[7]),
                mass: num(f[8]),
                eqt: num(f[9]),
                ecc: num(f[10]),
                st_teff: num(f[11]),
                st_rad: num(f[12]),
                st_mass: num(f[13]),
                spectype: f[14].to_string(),
                dist_pc: num(f[15]),
                derived: f[18].chars().next().unwrap_or(' '),
                home: false,
            });
        }
        for (name, period, smax, radius, mass, eqt) in HOME {
            all.push(Planet {
                name: name.to_string(),
                host: "the Sun".to_string(),
                method: Method::Other,
                year: None,
                period: Some(period),
                smax,
                radius: Some(radius),
                mass: Some(mass),
                eqt: Some(eqt),
                ecc: None,
                st_teff: Some(5772.0),
                st_rad: Some(1.0),
                st_mass: Some(1.0),
                spectype: "G2V".to_string(),
                dist_pc: Some(0.0),
                derived: ' ',
                home: true,
            });
        }
        Catalog { all }
    })
}

// ─────────────────────────── colour modes ────────────────────────────

pub const MODES: [&str; 6] = [
    "how it was found",
    "temperature",
    "year found",
    "distance",
    "host star",
    "density",
];

fn gradient(t: f64, stops: &[(u8, u8, u8)]) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
    let i = (t as usize).min(stops.len() - 2);
    let f = t - i as f64;
    let (a, b) = (stops[i], stops[i + 1]);
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * f) as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

const HEAT: [(u8, u8, u8); 5] = [
    (70, 90, 190),
    (60, 180, 200),
    (130, 220, 120),
    (255, 205, 80),
    (255, 85, 60),
];

pub fn heat(t: f64) -> (u8, u8, u8) {
    gradient(t, &HEAT)
}

/// A star's colour from its temperature, the same scale the rest of the
/// suite uses for stars.
pub fn star_rgb(teff: f64) -> (u8, u8, u8) {
    const STOPS: [(f64, (u8, u8, u8)); 7] = [
        (40000.0, (120, 150, 255)),
        (20000.0, (160, 195, 255)),
        (9700.0, (225, 235, 255)),
        (7200.0, (255, 245, 200)),
        (5800.0, (255, 215, 90)),
        (4400.0, (255, 150, 60)),
        (3000.0, (255, 90, 60)),
    ];
    let t = teff.clamp(3000.0, 40000.0);
    for w in STOPS.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t0 && t >= t1 {
            let f = (t0.ln() - t.ln()) / (t0.ln() - t1.ln());
            let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * f) as u8;
            return (mix(c0.0, c1.0), mix(c0.1, c1.1), mix(c0.2, c1.2));
        }
    }
    STOPS[0].1
}

/// Grey, for a planet the current mode knows nothing about.
pub const UNKNOWN_RGB: (u8, u8, u8) = (95, 95, 105);

pub fn rgb_for(p: &Planet, mode: usize) -> (u8, u8, u8) {
    if p.home {
        return (255, 255, 255);
    }
    match mode {
        // Equilibrium temperature: 100 K is Neptune, 2500 K is a world
        // with a rock-vapour atmosphere.
        1 => match p.eqt {
            Some(t) => heat(((t - 100.0) / 2400.0).clamp(0.0, 1.0)),
            None => UNKNOWN_RGB,
        },
        // The field is thirty years old and most of it is the last ten.
        2 => match p.year {
            Some(y) => heat(((y as f64 - 1992.0) / 34.0).clamp(0.0, 1.0)),
            None => UNKNOWN_RGB,
        },
        // Distance, log scale: 1 pc to 10 kpc, which reaches the
        // galactic bulge where the microlensing finds are.
        3 => match p.dist_pc {
            Some(d) if d > 0.0 => heat((d.log10() / 4.0).clamp(0.0, 1.0)),
            _ => UNKNOWN_RGB,
        },
        4 => match p.st_teff {
            Some(t) => star_rgb(t),
            None => UNKNOWN_RGB,
        },
        // Density: under 1 is a puffball, 5.5 is Earth, over 8 is iron.
        5 => match p.density() {
            Some(d) => heat((d / 9.0).clamp(0.0, 1.0)),
            _ => UNKNOWN_RGB,
        },
        _ => p.method.rgb(),
    }
}

/// The value the current colour mode is showing, spelled out.
pub fn mode_value(p: &Planet, mode: usize) -> String {
    match mode {
        1 => p.eqt.map(|t| format!("{t:.0} K")).unwrap_or_else(|| "—".into()),
        2 => p.year.map(|y| y.to_string()).unwrap_or_else(|| "—".into()),
        3 => p
            .dist_ly()
            .map(|d| format!("{d:.0} ly"))
            .unwrap_or_else(|| "—".into()),
        4 => p
            .st_teff
            .map(|t| format!("{t:.0} K"))
            .unwrap_or_else(|| "—".into()),
        5 => p
            .density()
            .map(|d| format!("{d:.2} g/cm³"))
            .unwrap_or_else(|| "—".into()),
        _ => p.method.label().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_loads() {
        let c = catalog();
        assert!(c.all.len() > 6300, "only {}", c.all.len());
        assert_eq!(c.all.iter().filter(|p| p.home).count(), 8);
        assert!(c.all.iter().filter(|p| p.radius.is_some()).count() > 6200);
        // The two methods that see what transits cannot: both are in.
        assert!(c.all.iter().filter(|p| p.method == Method::Microlensing).count() > 250);
        assert!(c.all.iter().filter(|p| p.method == Method::Imaging).count() > 80);
    }

    /// Values every exoplanet person knows by heart.
    #[test]
    fn the_famous_ones_are_right() {
        let c = catalog();
        let by = |n: &str| &c.all[c.find(n).unwrap()];
        // The first planet found around a sun-like star, 1995.
        let peg = by("51 Peg b");
        assert_eq!(peg.year, Some(1995));
        assert!((peg.period.unwrap() - 4.23).abs() < 0.01);
        assert_eq!(peg.method, Method::RadialVelocity);
        // Our nearest neighbour.
        let prox = by("Proxima Cen b");
        assert!((prox.dist_ly().unwrap() - 4.24).abs() < 0.1);
        // TRAPPIST-1 has seven, and e is the Earth-sized one.
        let t1e = by("TRAPPIST-1 e");
        assert_eq!(c.system("TRAPPIST-1").len(), 7);
        assert!((t1e.radius.unwrap() - 0.92).abs() < 0.02);
    }

    /// Earth is the unit, so its density has to come out at 5.5 g/cm³
    /// and its gravity at 1.
    #[test]
    fn earth_is_the_yardstick() {
        let c = catalog();
        let earth = &c.all[c.find("Earth").unwrap()];
        assert!((earth.density().unwrap() - 5.513).abs() < 0.01);
        assert!((earth.gravity().unwrap() - 1.0).abs() < 0.001);
        assert!((earth.insolation().unwrap() - 1.0).abs() < 0.01);
        assert_eq!(earth.kind(), "Earth-sized");
        assert!(earth.in_hz());
        // Saturn floats, and Mars is out in the cold.
        let saturn = &c.all[c.find("Saturn").unwrap()];
        assert!(saturn.density().unwrap() < 0.8);
        assert!(!c.all[c.find("Mars").unwrap()].in_hz());
    }

    /// The zone tracks the star: a red dwarf's is right up against it.
    #[test]
    fn the_zone_scales_with_the_star() {
        let c = catalog();
        let t1e = &c.all[c.find("TRAPPIST-1 e").unwrap()];
        let (inner, outer) = t1e.hz().unwrap();
        assert!(inner < 0.05 && outer < 0.1, "zone {inner}–{outer} AU");
        assert!(t1e.in_hz());
        // TRAPPIST-1 b is the scorched one, well inside the zone.
        assert!(!c.all[c.find("TRAPPIST-1 b").unwrap()].in_hz());
    }

    /// Stars are thousands of kelvin, never tens. A stored temperature
    /// that had lost its trailing zeroes would land here.
    #[test]
    fn the_stars_are_hot() {
        let c = catalog();
        let prox = &c.all[c.find("Proxima Cen b").unwrap()];
        assert!((prox.st_teff.unwrap() - 2900.0).abs() < 1.0);
        let cold = c.all.iter().filter(|p| p.st_teff.is_some_and(|t| t < 1000.0)).count();
        assert!(cold < 10, "{cold} hosts under 1000 K");
        assert!(c.all.iter().all(|p| p.st_teff.is_none_or(|t| t < 60000.0)));
    }

    #[test]
    fn transit_found_most_of_them() {
        let c = catalog();
        let transits = c.all.iter().filter(|p| p.method == Method::Transit).count();
        assert!(transits > c.all.len() / 2, "only {transits} transits");
    }

    /// Every planet has a place on the x-axis, measured or from Kepler.
    #[test]
    fn every_planet_has_an_orbit() {
        let c = catalog();
        assert!(c.all.iter().all(|p| p.smax > 0.0));
        let kepler = c.all.iter().filter(|p| p.derived != ' ').count();
        assert!(kepler > 500, "only {kepler} filled in");
        // A filled-in value has to satisfy Kepler's third law.
        for p in c.all.iter().filter(|p| p.derived != ' ') {
            let (Some(per), Some(m)) = (p.period, p.st_mass) else { continue };
            let want = 365.25 * (p.smax.powi(3) / m).sqrt();
            // Loose enough for the rounding in the stored table, tight
            // enough to catch a value that came from somewhere else.
            assert!((per - want).abs() / want < 1e-3, "{}: {per} vs {want}", p.name);
        }
    }

    /// The stand-in size for the few dozen planets weighed but never
    /// measured across.
    #[test]
    fn a_mass_can_stand_in_for_a_size() {
        assert!((mass_to_radius(1.0) - 1.0).abs() < 0.02);
        // A Jupiter mass lands near a Jupiter, a little over: the fit
        // averages in the puffed-up hot ones.
        assert!((mass_to_radius(317.8) - 11.2).abs() < 3.0);
        // Ten Jupiters is no bigger. Past this point the extra mass just
        // squeezes.
        assert!(mass_to_radius(3178.0) < mass_to_radius(317.8));
        // No jumps at the joins.
        for m in [2.04, 132.0] {
            let (lo, hi) = (mass_to_radius(m * 0.999), mass_to_radius(m * 1.001));
            assert!((lo - hi).abs() < 0.02, "step at {m}: {lo} to {hi}");
        }
        // Every planet on the diagram has a size to plot.
        assert!(catalog().all.iter().all(|p| p.plot_radius() > 0.0));
    }

    #[test]
    fn search_takes_a_host_too() {
        let c = catalog();
        assert!(c.find("trappist-1").is_some());
        assert!(c.find("KEPLER-452 B").is_some());
        assert!(c.find("no such world").is_none());
    }
}
