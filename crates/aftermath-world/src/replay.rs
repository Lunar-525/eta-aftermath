use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use aftermath_world::events::{Event, write_csv};
use aftermath_world::social::{COOPERATIVE, COUNCIL, EXCHANGE};
use aftermath_world::world::{Account, World};
use aftermath_world::{DayReport, Simulation};

pub struct Replay {
    path: PathBuf,
    seed: u64,
    use_familiarity: bool,
    frames: String,
    food_quality: String,
    plots: String,
    organizations: String,
    residents: String,
    relationship_deltas: String,
    relationship_snapshots: String,
}

impl Replay {
    pub fn new(path: PathBuf, seed: u64, opening: &World, use_familiarity: bool) -> Self {
        use std::fmt::Write;

        let mut plots = String::new();
        for plot in opening.plots() {
            let _ = writeln!(
                plots,
                "{},{},{},{},{},{}",
                plot.col,
                plot.row,
                plot.wood,
                plot.steel,
                plot.wood_capacity,
                u8::from(plot.accessible),
            );
        }
        let mut organizations = String::new();
        for (organization, members) in [
            ("council", COUNCIL.as_slice()),
            ("cooperative", COOPERATIVE.as_slice()),
            ("exchange", EXCHANGE.as_slice()),
        ] {
            for &id in members {
                let _ = writeln!(organizations, "{organization},{id}");
            }
        }
        let mut residents = String::new();
        for resident in &opening.residents {
            let _ = writeln!(residents, "{},{}", resident.id, resident.name);
        }
        Self {
            path,
            seed,
            use_familiarity,
            frames: String::new(),
            food_quality: String::new(),
            plots,
            organizations,
            residents,
            relationship_deltas: String::new(),
            relationship_snapshots: String::new(),
        }
    }

    pub fn push(&mut self, report: &DayReport, simulation: &Simulation) {
        use std::fmt::Write;

        let world = simulation.world();
        for crew in &report.completed_crews {
            let _ = write!(self.relationship_deltas, "{}", report.day);
            for resident in crew {
                let _ = write!(self.relationship_deltas, ",{resident}");
            }
            let _ = writeln!(self.relationship_deltas);
        }
        if report.day.is_multiple_of(30) {
            let _ = writeln!(self.relationship_snapshots, "{}", report.day);
            for (left, right, pair) in simulation.relationships() {
                let _ = writeln!(
                    self.relationship_snapshots,
                    "{left},{right},{},{}",
                    pair.completed_together, pair.last_day
                );
            }
        }

        let _ = writeln!(
            self.frames,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            report.day,
            report.weather.name(),
            report.population,
            world.balance(Account::Food),
            world.balance(Account::Water),
            world.balance(Account::Fuel),
            world.balance(Account::Wood),
            world.balance(Account::Steel),
            world.pump_condition,
            report.food_shortfall,
            report.water_shortfall,
            report.accepted_shifts,
            report.completed_shifts,
            report.unfilled_shifts,
            world.balance(Account::Medicine),
            report.medical_capacity,
            report.treated,
            report.waiting_for_care,
            report.kits_prepared,
            world.balance(Account::Cloth),
            world.balance(Account::Antiseptic),
            report.grid.renewable,
            report.grid.backup,
            report.grid.requested,
            report.grid.delivered,
            report.grid.rated,
            world.granary_condition,
            world.granary_capacity(),
            report.water_requested,
            world.water_quality(),
            report.grid.voltage,
            report.grid.frequency_centihz,
        );
        let amounts = world.food_quality();
        let mut fields = amounts.iter().flatten();
        if let Some(first) = fields.next() {
            let _ = write!(self.food_quality, "{first}");
            for value in fields {
                let _ = write!(self.food_quality, ",{value}");
            }
            self.food_quality.push('\n');
        }
    }

    pub fn write(self, events: &[Event]) -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)?;
        let mut output = BufWriter::new(file);
        let (head, body) = include_str!("replay/index.html")
            .split_once("</head>")
            .expect("replay template has a closing head tag");
        output.write_all(head.as_bytes())?;
        output.write_all(b"<style>\n")?;
        write_fonts(&mut output)?;
        output.write_all(include_bytes!("replay/style.css"))?;
        output.write_all(b"\n</style>\n</head>")?;
        output.write_all(body.as_bytes())?;
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"seed\">{}</script>",
            self.seed
        )?;
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"frames\">\n{}</script>",
            self.frames
        )?;
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"plots\">\n{}</script>",
            self.plots
        )?;
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"food-quality\">\n{}</script>",
            self.food_quality
        )?;
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"organizations\">\n{}</script>",
            self.organizations
        )?;
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"residents\">\n{}</script>",
            self.residents
        )?;
        for (id, data) in [
            ("relationship-snapshots", &self.relationship_snapshots),
            ("relationship-deltas", &self.relationship_deltas),
        ] {
            writeln!(
                output,
                "<script type=\"text/plain\" id=\"{id}\">\n{data}</script>"
            )?;
        }
        writeln!(
            output,
            "<script type=\"text/plain\" id=\"use-familiarity\">{}</script>",
            self.use_familiarity
        )?;
        output.write_all(b"<script type=\"text/plain\" id=\"units\">\n")?;
        for account in Account::ALL {
            let (scale, unit) = account.measurement();
            writeln!(output, "{},{scale},{unit}", account.name())?;
        }
        output.write_all(b"</script>\n")?;
        output.write_all(b"<script type=\"text/plain\" id=\"photos\">\n")?;
        for (place, data) in [
            (
                "太湖饭店",
                include_bytes!("replay/photos/taihu-hotel.jpg").as_slice(),
            ),
            (
                "渤公岛",
                include_bytes!("replay/photos/bogong-island.jpg").as_slice(),
            ),
            (
                "断桥码头",
                include_bytes!("replay/photos/duanqiao-pier.jpg").as_slice(),
            ),
            (
                "滨湖残料场",
                include_bytes!("replay/photos/binhu-salvage.jpg").as_slice(),
            ),
            (
                "西线封锁桩",
                include_bytes!("replay/photos/west-blockade.jpg").as_slice(),
            ),
            (
                "堤上公共灶",
                include_bytes!("replay/photos/dike-kitchen.jpg").as_slice(),
            ),
            (
                "残堤苗圃与农田",
                include_bytes!("replay/photos/dike-farm.jpg").as_slice(),
            ),
        ] {
            write!(output, "{place}:")?;
            write_base64(&mut output, data)?;
            writeln!(output)?;
        }
        output.write_all(b"</script>\n")?;
        output.write_all(b"<script type=\"text/plain\" id=\"events\">\n")?;
        write_csv(&mut output, events)?;
        output.write_all(b"</script>\n<script>\n")?;
        output.write_all(include_bytes!("replay/geography.js"))?;
        output.write_all(b"\n")?;
        output.write_all(include_bytes!("replay/map.js"))?;
        output.write_all(b"\n</script>\n</body>\n</html>\n")?;
        output.flush()
    }
}

fn write_fonts(output: &mut impl Write) -> io::Result<()> {
    for license in [
        include_str!("replay/fonts/OFL-Adobe.txt"),
        include_str!("replay/fonts/OFL-Oswald.txt"),
    ] {
        writeln!(output, "/*\n{license}\n*/")?;
    }
    for (family, weights, data) in [
        (
            "Oswald",
            "200 700",
            include_bytes!("replay/fonts/oswald.woff").as_slice(),
        ),
        (
            "Aftermath Han",
            "400 700",
            include_bytes!("replay/fonts/han-medium.woff").as_slice(),
        ),
        (
            "Aftermath Han",
            "800 900",
            include_bytes!("replay/fonts/han-heavy.woff").as_slice(),
        ),
    ] {
        write!(
            output,
            "@font-face {{ font-family: '{family}'; font-weight: {weights}; font-display: swap; src: url(data:font/woff;base64,"
        )?;
        write_base64(output, data)?;
        writeln!(output, ") format('woff'); }}")?;
    }
    Ok(())
}

fn write_base64(output: &mut impl Write, data: &[u8]) -> io::Result<()> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in data.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let mut encoded = [
            ALPHABET[usize::from(first >> 2)],
            ALPHABET[usize::from((first & 3) << 4 | second >> 4)],
            ALPHABET[usize::from((second & 15) << 2 | third >> 6)],
            ALPHABET[usize::from(third & 63)],
        ];
        for padding in &mut encoded[chunk.len() + 1..] {
            *padding = b'=';
        }
        output.write_all(&encoded)?;
    }
    Ok(())
}
