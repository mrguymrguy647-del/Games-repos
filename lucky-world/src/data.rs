//! The wheels, maps and names, read from the TOML files in `data/`.
//!
//! Everything a designer tunes lives in those files; this module reads them,
//! turns them into plain structs and checks them, so a typo is one sentence
//! naming the file and the wheel instead of a strange spin later.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use duckforge::prelude::Color;
use serde::Deserialize;

/// Where the data files are, relative to the folder the game runs from.
pub const DATA_DIR: &str = "data";

// ---------------------------------------------------------------------------
// The files as they are written
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WheelsFile {
    min_share: f64,
    slices: usize,
    palette: Vec<String>,
    #[serde(rename = "round")]
    rounds: Vec<RoundDef>,
    #[serde(rename = "wheel")]
    wheels: Vec<WheelFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundDef {
    pub number: usize,
    pub title: String,
    pub tagline: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WheelFile {
    id: String,
    round: usize,
    title: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    log: bool,
    #[serde(default, rename = "option")]
    options: Vec<OptionFile>,
    #[serde(default, rename = "rule")]
    rules: Vec<RuleFile>,
    #[serde(default, rename = "follow")]
    follows: Vec<FollowFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionFile {
    id: String,
    label: String,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    weight: Option<f64>,
    #[serde(default)]
    share: Option<f64>,
    #[serde(default)]
    level: Option<f64>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    unique: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Conditions {
    One(String),
    All(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    #[serde(rename = "if")]
    cond: Conditions,
    #[serde(default)]
    times: BTreeMap<String, f64>,
    #[serde(default)]
    only: Vec<String>,
    #[serde(default)]
    shift: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FollowFile {
    from: BTreeMap<String, f64>,
    #[serde(default)]
    shift: f64,
    spread: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapsFile {
    #[serde(rename = "map")]
    maps: Vec<MapFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    islands: bool,
    rivals: usize,
    #[serde(rename = "continent")]
    continents: Vec<ContinentFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContinentFile {
    id: String,
    label: String,
    weight: f64,
    #[serde(default)]
    tags: Vec<String>,
    style: String,
    #[serde(default)]
    color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NamesFile {
    middle_chance: f64,
    style: BTreeMap<String, NameStyle>,
    forms: BTreeMap<String, Vec<String>>,
    flags: FlagsFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameStyle {
    pub start: Vec<String>,
    pub middle: Vec<String>,
    pub end: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FlagsFile {
    colors: Vec<String>,
    layouts: BTreeMap<String, f64>,
    emblem_chance: f64,
}

// ---------------------------------------------------------------------------
// The data as the game uses it
// ---------------------------------------------------------------------------

/// Everything in `data/`.
#[derive(Debug, Clone)]
pub struct Data {
    pub min_share: f64,
    pub slices: usize,
    pub rounds: Vec<RoundDef>,
    pub wheels: Vec<Wheel>,
    pub maps: Vec<MapDef>,
    pub names: Names,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Lands on one of its options.
    Pick,
    /// Lands on a band, then rolls an exact number inside it.
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Km2,
    People,
    Usd,
    Percent,
    Hdi,
}

#[derive(Debug, Clone)]
pub struct Wheel {
    pub id: String,
    pub round: usize,
    pub title: String,
    pub kind: Kind,
    pub unit: Unit,
    pub log: bool,
    /// Options come from the map (the Continent wheel).
    pub from_map: bool,
    pub options: Vec<WheelOption>,
    pub rules: Vec<Rule>,
    pub follows: Vec<Follow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unique {
    World,
    Continent,
}

#[derive(Debug, Clone)]
pub struct WheelOption {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    /// Relative weight (ignored when `share` is set).
    pub weight: f64,
    /// A fixed part of the wheel, not moved by follows.
    pub share: Option<f64>,
    pub level: f64,
    pub min: f64,
    pub max: f64,
    pub tags: Vec<String>,
    pub color: Color,
    pub unique: Option<Unique>,
}

/// One condition of a rule's `if`.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub negated: bool,
    /// Index of the wheel it looks at.
    pub wheel: usize,
    pub test: Test,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    /// Landed on one of these option ids.
    Is(Vec<String>),
    /// The option landed on has one of these tags.
    Tagged(Vec<String>),
    /// The exact value (number wheels) or level (pick wheels) is below.
    Below(f64),
    Above(f64),
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub conditions: Vec<Condition>,
    /// (option id or "#tag", factor)
    pub times: Vec<(String, f64)>,
    pub only: Vec<String>,
    pub shift: f64,
}

#[derive(Debug, Clone)]
pub struct Follow {
    /// (wheel index, scale)
    pub from: Vec<(usize, f64)>,
    pub shift: f64,
    pub spread: f64,
}

#[derive(Debug, Clone)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rivals: usize,
    pub continents: Vec<Continent>,
}

#[derive(Debug, Clone)]
pub struct Continent {
    pub id: String,
    pub label: String,
    pub weight: f64,
    pub tags: Vec<String>,
    pub style: String,
    pub color: Color,
}

#[derive(Debug, Clone)]
pub struct Names {
    pub middle_chance: f64,
    pub styles: BTreeMap<String, NameStyle>,
    pub forms: BTreeMap<String, Vec<String>>,
    pub flag_colors: Vec<Color>,
    pub flag_layouts: Vec<(String, f64)>,
    pub emblem_chance: f64,
}

/// A mistake in a data file, as one sentence.
#[derive(Debug, Clone, PartialEq)]
pub struct DataError(pub String);

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DataError {}

fn err<T>(text: impl Into<String>) -> Result<T, DataError> {
    Err(DataError(text.into()))
}

/// The flag layouts the game knows how to draw.
pub const FLAG_LAYOUTS: [&str; 7] = [
    "horizontal",
    "vertical",
    "bicolor",
    "cross",
    "canton",
    "diagonal",
    "circle",
];

/// Reads `#rrggbb`.
pub fn parse_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::from_hex(value))
}

impl Data {
    /// Reads `data/wheels.toml`, `data/maps.toml` and `data/names.toml`.
    pub fn load() -> Result<Data, DataError> {
        Self::load_from(Path::new(DATA_DIR))
    }

    pub fn load_from(dir: &Path) -> Result<Data, DataError> {
        let read = |name: &str| -> Result<String, DataError> {
            let path = dir.join(name);
            std::fs::read_to_string(&path).map_err(|e| {
                DataError(format!(
                    "couldn't read {} ({e}); run the game from the lucky-world folder",
                    path.display()
                ))
            })
        };
        Self::parse(&read("wheels.toml")?, &read("maps.toml")?, &read("names.toml")?)
    }

    /// The three files' text, for tests and for files built into the game.
    pub fn parse(wheels: &str, maps: &str, names: &str) -> Result<Data, DataError> {
        let wheels: WheelsFile = toml::from_str(wheels)
            .map_err(|e| DataError(format!("data/wheels.toml: {}", one_line(&e.to_string()))))?;
        let maps: MapsFile = toml::from_str(maps)
            .map_err(|e| DataError(format!("data/maps.toml: {}", one_line(&e.to_string()))))?;
        let names: NamesFile = toml::from_str(names)
            .map_err(|e| DataError(format!("data/names.toml: {}", one_line(&e.to_string()))))?;
        build(wheels, maps, names)
    }

    pub fn wheel_index(&self, id: &str) -> Option<usize> {
        self.wheels.iter().position(|w| w.id == id)
    }

    /// Index of the wheel with this id. Panics, naming it, if there's none:
    /// for ids the game's own code uses, which `cargo test` checks.
    pub fn wheel(&self, id: &str) -> usize {
        self.wheel_index(id)
            .unwrap_or_else(|| panic!("data/wheels.toml has no wheel \"{id}\""))
    }

    /// The wheels of one round (1 to 4), as indexes.
    pub fn round_wheels(&self, round: usize) -> Vec<usize> {
        (0..self.wheels.len())
            .filter(|&i| self.wheels[i].round == round)
            .collect()
    }

    pub fn round(&self, number: usize) -> &RoundDef {
        self.rounds
            .iter()
            .find(|r| r.number == number)
            .expect("rounds are checked when loading")
    }

    /// A wheel's options on a map: the Continent wheel's come from the map.
    pub fn options_on(&self, wheel: usize, map: usize) -> Vec<WheelOption> {
        let w = &self.wheels[wheel];
        if !w.from_map {
            return w.options.clone();
        }
        self.maps[map]
            .continents
            .iter()
            .enumerate()
            .map(|(i, c)| WheelOption {
                id: c.id.clone(),
                label: c.label.clone(),
                detail: None,
                weight: c.weight,
                share: None,
                level: i as f64,
                min: 0.0,
                max: 0.0,
                tags: c.tags.clone(),
                color: c.color,
                unique: None,
            })
            .collect()
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build(file: WheelsFile, maps: MapsFile, names: NamesFile) -> Result<Data, DataError> {
    let w = "data/wheels.toml";
    if !(0.0..0.5).contains(&file.min_share) {
        return err(format!("{w}: min_share must be between 0 and 0.5"));
    }
    if file.slices < 2 {
        return err(format!("{w}: slices must be at least 2"));
    }
    let mut palette = Vec::new();
    for c in &file.palette {
        palette.push(parse_color(c).ok_or_else(|| {
            DataError(format!("{w}: palette colour \"{c}\" isn't written like \"#e8505b\""))
        })?);
    }
    if palette.is_empty() {
        return err(format!("{w}: the palette needs at least one colour"));
    }
    for n in 1..=4 {
        if !file.rounds.iter().any(|r| r.number == n) {
            return err(format!("{w}: there's no [[round]] with number = {n}"));
        }
    }

    // Ids first, so rules can name any earlier wheel.
    let ids: Vec<String> = file.wheels.iter().map(|w| w.id.clone()).collect();
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            return err(format!("{w}: two wheels are called \"{id}\""));
        }
    }

    let mut wheels: Vec<Wheel> = Vec::new();
    for (index, wf) in file.wheels.into_iter().enumerate() {
        let here = format!("{w}, wheel \"{}\"", wf.id);
        if !(1..=4).contains(&wf.round) {
            return err(format!("{here}: round must be 1, 2, 3 or 4"));
        }
        if let Some(prev) = wheels.last() {
            if wf.round < prev.round {
                return err(format!(
                    "{here}: wheels must be in spin order, but round {} comes after round {}",
                    wf.round, prev.round
                ));
            }
        }
        let from_map = match wf.source.as_deref() {
            None => false,
            Some("map") => true,
            Some(other) => return err(format!("{here}: source = \"{other}\" isn't known (only \"map\")")),
        };
        let kind = match wf.kind.as_deref() {
            None | Some("pick") => Kind::Pick,
            Some("number") => Kind::Number,
            Some(other) => {
                return err(format!("{here}: kind = \"{other}\" isn't known (\"pick\" or \"number\")"));
            }
        };
        let unit = match wf.unit.as_deref() {
            None => Unit::None,
            Some("km2") => Unit::Km2,
            Some("people") => Unit::People,
            Some("usd") => Unit::Usd,
            Some("percent") => Unit::Percent,
            Some("hdi") => Unit::Hdi,
            Some(other) => return err(format!("{here}: unit = \"{other}\" isn't known")),
        };
        if from_map && !wf.options.is_empty() {
            return err(format!("{here}: a wheel with source = \"map\" takes its options from maps.toml, so it can't list its own"));
        }
        if !from_map && wf.options.len() < 2 {
            return err(format!("{here}: a wheel needs at least two options"));
        }

        let mut options = Vec::new();
        for (oi, of) in wf.options.into_iter().enumerate() {
            let at = format!("{here}, option \"{}\"", of.id);
            if options.iter().any(|o: &WheelOption| o.id == of.id) {
                return err(format!("{here}: two options are called \"{}\"", of.id));
            }
            let (weight, share) = match (of.weight, of.share) {
                (Some(_), Some(_)) => return err(format!("{at}: give it a weight or a share, not both")),
                (None, None) => return err(format!("{at}: it needs a weight (or a share)")),
                (Some(weight), None) if weight < 0.0 => return err(format!("{at}: weight can't be below 0")),
                (Some(weight), None) => (weight, None),
                (None, Some(share)) if !(0.0..1.0).contains(&share) => {
                    return err(format!("{at}: share must be between 0 and 1 (0.04 is 4%)"));
                }
                (None, Some(share)) => (0.0, Some(share)),
            };
            let (min, max) = match kind {
                Kind::Number => match (of.min, of.max) {
                    (Some(min), Some(max)) if min <= max && (!wf.log || min > 0.0) => (min, max),
                    (Some(_), Some(_)) if wf.log => {
                        return err(format!("{at}: needs 0 < min <= max (the wheel is log = true)"));
                    }
                    (Some(_), Some(_)) => return err(format!("{at}: min is bigger than max")),
                    _ => return err(format!("{at}: a number wheel's options need min and max")),
                },
                Kind::Pick => (0.0, 0.0),
            };
            let level = match of.level {
                Some(level) => level,
                None if kind == Kind::Number && wf.log => (min.log10() + max.log10()) / 2.0,
                None => oi as f64,
            };
            let color = match &of.color {
                Some(c) => parse_color(c)
                    .ok_or_else(|| DataError(format!("{at}: colour \"{c}\" isn't written like \"#e8505b\"")))?,
                None => palette[oi % palette.len()],
            };
            let unique = match of.unique.as_deref() {
                None => None,
                Some("world") => Some(Unique::World),
                Some("continent") => Some(Unique::Continent),
                Some(other) => return err(format!("{at}: unique = \"{other}\" isn't known (\"world\" or \"continent\")")),
            };
            options.push(WheelOption {
                id: of.id,
                label: of.label,
                detail: of.detail,
                weight,
                share,
                level,
                min,
                max,
                tags: of.tags,
                color,
                unique,
            });
        }

        // Which wheels this one may look at: those spun before it.
        let earlier = &ids[..index];
        let find_earlier = |name: &str, what: &str| -> Result<usize, DataError> {
            match earlier.iter().position(|id| id == name) {
                Some(i) => Ok(i),
                None if ids.iter().any(|id| id == name) => err(format!(
                    "{here}: its {what} looks at wheel \"{name}\", which is spun after it; rules can only look at earlier wheels"
                )),
                None => err(format!("{here}: its {what} names a wheel \"{name}\" that doesn't exist")),
            }
        };

        let mut rules = Vec::new();
        for rf in wf.rules {
            let texts = match rf.cond {
                Conditions::One(t) => vec![t],
                Conditions::All(ts) => ts,
            };
            let mut conditions = Vec::new();
            for text in &texts {
                conditions.push(parse_condition(text, &find_earlier, &here)?);
            }
            for (name, factor) in &rf.times {
                if *factor < 0.0 {
                    return err(format!("{here}: a rule multiplies \"{name}\" by {factor}, below 0"));
                }
                if !from_map && !name.starts_with('#') && !options.iter().any(|o| &o.id == name) {
                    return err(format!("{here}: a rule's times names \"{name}\", which isn't one of its options"));
                }
            }
            for name in &rf.only {
                if !from_map && !options.iter().any(|o| &o.id == name) {
                    return err(format!("{here}: a rule's only names \"{name}\", which isn't one of its options"));
                }
            }
            rules.push(Rule {
                conditions,
                times: rf.times.into_iter().collect(),
                only: rf.only,
                shift: rf.shift,
            });
        }

        let mut follows = Vec::new();
        for ff in wf.follows {
            if ff.spread <= 0.0 {
                return err(format!("{here}: a follow's spread must be above 0"));
            }
            let mut from = Vec::new();
            for (name, scale) in ff.from {
                from.push((find_earlier(&name, "follow")?, scale));
            }
            follows.push(Follow {
                from,
                shift: ff.shift,
                spread: ff.spread,
            });
        }
        if !follows.is_empty() && !options.iter().any(|o| o.share.is_none()) {
            return err(format!("{here}: it has a follow, but every option has a fixed share"));
        }

        wheels.push(Wheel {
            id: wf.id,
            round: wf.round,
            title: wf.title,
            kind,
            unit,
            log: wf.log,
            from_map,
            options,
            rules,
            follows,
        });
    }
    if wheels.len() != 16 {
        return err(format!("{w}: LUCKY WORLD has 16 wheels, but the file has {}", wheels.len()));
    }
    if !wheels[0].from_map {
        return err(format!("{w}: the first wheel must be the Continent wheel (source = \"map\")"));
    }

    // Maps.
    let m = "data/maps.toml";
    if maps.maps.len() != 3 {
        return err(format!("{m}: LUCKY WORLD has 3 maps, but the file has {}", maps.maps.len()));
    }
    let mut map_defs = Vec::new();
    for mf in maps.maps {
        let here = format!("{m}, map \"{}\"", mf.id);
        if mf.continents.len() < 2 {
            return err(format!("{here}: a map needs at least two continents"));
        }
        let mut continents = Vec::new();
        for (ci, cf) in mf.continents.into_iter().enumerate() {
            if cf.weight <= 0.0 {
                return err(format!("{here}, continent \"{}\": weight must be above 0", cf.id));
            }
            if !names.style.contains_key(&cf.style) {
                return err(format!(
                    "{here}, continent \"{}\": style \"{}\" isn't in data/names.toml",
                    cf.id, cf.style
                ));
            }
            let mut tags = cf.tags;
            if mf.islands && !tags.iter().any(|t| t == "island") {
                tags.push("island".to_string());
            }
            let color = match &cf.color {
                Some(c) => parse_color(c).ok_or_else(|| DataError(format!("{here}: bad colour \"{c}\"")))?,
                None => palette[ci % palette.len()],
            };
            continents.push(Continent {
                id: cf.id,
                label: cf.label,
                weight: cf.weight,
                tags,
                style: cf.style,
                color,
            });
        }
        map_defs.push(MapDef {
            id: mf.id,
            name: mf.name,
            description: mf.description,
            rivals: mf.rivals,
            continents,
        });
    }

    // Names.
    let n = "data/names.toml";
    for (name, style) in &names.style {
        if style.start.is_empty() || style.end.is_empty() {
            return err(format!("{n}: style \"{name}\" needs start and end syllables"));
        }
    }
    let ideology = wheels.iter().find(|w| w.id == "ideology");
    if let Some(ideology) = ideology {
        for o in &ideology.options {
            match names.forms.get(&o.id) {
                Some(forms) if !forms.is_empty() && forms.iter().all(|f| f.contains("{}")) => {}
                _ => {
                    return err(format!(
                        "{n}: [forms] needs {} = [\"... {{}} ...\"] for the ideology option \"{}\"",
                        o.id, o.id
                    ));
                }
            }
        }
    }
    let mut flag_colors = Vec::new();
    for c in &names.flags.colors {
        flag_colors.push(parse_color(c).ok_or_else(|| DataError(format!("{n}: bad flag colour \"{c}\"")))?);
    }
    if flag_colors.len() < 3 {
        return err(format!("{n}: flags need at least 3 colours"));
    }
    for layout in names.flags.layouts.keys() {
        if !FLAG_LAYOUTS.contains(&layout.as_str()) {
            return err(format!(
                "{n}: flag layout \"{layout}\" isn't known (try {})",
                FLAG_LAYOUTS.join(", ")
            ));
        }
    }

    // Every option and tag a condition names must exist, or a typo would
    // quietly make a rule never fire.
    for wheel in &wheels {
        for rule in &wheel.rules {
            for c in &rule.conditions {
                let target = &wheels[c.wheel];
                let (ids, tags): (Vec<&String>, Vec<&String>) = if target.from_map {
                    let all = map_defs.iter().flat_map(|m| &m.continents);
                    (all.clone().map(|c| &c.id).collect(), all.flat_map(|c| &c.tags).collect())
                } else {
                    (
                        target.options.iter().map(|o| &o.id).collect(),
                        target.options.iter().flat_map(|o| &o.tags).collect(),
                    )
                };
                let (names, known, what) = match &c.test {
                    Test::Is(names) => (names, &ids, "option"),
                    Test::Tagged(names) => (names, &tags, "tag"),
                    _ => continue,
                };
                for name in names {
                    if !known.contains(&name) {
                        return err(format!(
                            "{w}, wheel \"{}\": a rule looks for the {what} \"{name}\" on wheel \"{}\", which has none by that name",
                            wheel.id, target.id
                        ));
                    }
                }
            }
        }
    }

    Ok(Data {
        min_share: file.min_share,
        slices: file.slices,
        rounds: file.rounds,
        wheels,
        maps: map_defs,
        names: Names {
            middle_chance: names.middle_chance,
            styles: names.style,
            forms: names.forms,
            flag_colors,
            flag_layouts: names.flags.layouts.into_iter().collect(),
            emblem_chance: names.flags.emblem_chance,
        },
    })
}

fn parse_condition(
    text: &str,
    find_earlier: &dyn Fn(&str, &str) -> Result<usize, DataError>,
    here: &str,
) -> Result<Condition, DataError> {
    let (negated, body) = match text.trim().strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, text.trim()),
    };
    let Some(at) = body.find(['=', '#', '<', '>']) else {
        return err(format!(
            "{here}: the condition \"{text}\" needs =, #, < or > (like \"coast=landlocked\")"
        ));
    };
    let (name, rest) = body.split_at(at);
    let op = &rest[..1];
    let value = rest[1..].trim();
    let wheel = find_earlier(name.trim(), "rule")?;
    let list = || value.split('|').map(|s| s.trim().to_string()).collect::<Vec<_>>();
    let number = || -> Result<f64, DataError> {
        value
            .replace('_', "")
            .parse()
            .map_err(|_| DataError(format!("{here}: \"{value}\" in \"{text}\" isn't a number")))
    };
    let test = match op {
        "=" => Test::Is(list()),
        "#" => Test::Tagged(list()),
        "<" => Test::Below(number()?),
        _ => Test::Above(number()?),
    };
    Ok(Condition {
        negated,
        wheel,
        test,
    })
}
