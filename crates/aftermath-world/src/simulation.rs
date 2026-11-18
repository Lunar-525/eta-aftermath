use std::ops::Range;

use crate::events::{Change, Event, Flow, Journal, Subject};
use crate::social::{
    COOPERATIVE, COUNCIL, EXCHANGE, FUEL_REVIEW, Relationship, Social, WATER_RESERVE, can_attend,
    supports_limit,
};
use crate::world::{
    Account, FUEL_BATCH, FUEL_YARD_TARGET, RESIDENT_COUNT, Random, Resident, Trade, World,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
}

impl Weather {
    pub fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn name(self) -> &'static str {
        match self {
            Self::Spring => "spring",
            Self::Summer => "summer",
            Self::Autumn => "autumn",
            Self::Winter => "winter",
        }
    }

    /// Farm output per shift. Fishing stays at six on clear days and four in
    /// rain because the lake is workable year round.
    fn farm_yield(self, weather: Weather) -> u32 {
        match (self, weather) {
            (Self::Spring, Weather::Clear) => 6,
            (Self::Spring, Weather::Rain) => 4,
            (Self::Summer, Weather::Clear) => 7,
            (Self::Summer, Weather::Rain) => 5,
            (Self::Autumn, Weather::Clear) => 8,
            (Self::Autumn, Weather::Rain) => 6,
            (Self::Winter, Weather::Clear) => 5,
            (Self::Winter, Weather::Rain) => 4,
        }
    }

    fn algae_penalty(self) -> u32 {
        match self {
            Self::Summer => ALGAE_QUALITY_PENALTY,
            _ => 0,
        }
    }

    fn water_load(self) -> u32 {
        match self {
            Self::Summer => WATER_LOAD + SUMMER_WATER_LOAD,
            _ => WATER_LOAD,
        }
    }
}

/// The settlement starts on 2051-04-01, so day one is April. Months are
/// zero-based Gregorian months and 2052 is a leap year.
fn month_length(year: u32, month: u32) -> u32 {
    match month {
        1 if year.is_multiple_of(4) => 29,
        1 => 28,
        3 | 5 | 8 | 10 => 30,
        _ => 31,
    }
}

fn month_of(day: u32) -> u32 {
    let mut offset = day.saturating_sub(1);
    let mut month = 3u32;
    let mut year = 2051u32;
    loop {
        let length = month_length(year, month);
        if offset < length {
            return month + 1;
        }
        offset -= length;
        month += 1;
        if month == 12 {
            month = 0;
            year += 1;
        }
    }
}

fn season_of(day: u32) -> Season {
    match month_of(day) {
        3..=5 => Season::Spring,
        6..=8 => Season::Summer,
        9..=11 => Season::Autumn,
        _ => Season::Winter,
    }
}

const RENEWABLE_RATED: u32 = 14;
const BACKUP_BATCH_LIMIT: u32 = 2;
const POWER_PER_BATCH: u32 = 4;
const ARCHIVE_LOAD: u32 = 1;
const KITCHEN_LOAD: u32 = 1;
const WATER_LOAD: u32 = 4;
const REPAIR_LOAD: u32 = 2;
const WATER_SHIFTS: usize = 14;
const FIELD_SHIFTS: usize = 13;
const FISHING_SHIFTS: usize = 7;
const RECOVERY_SHIFTS: usize = 2;
const CLOTH_SHIFTS: usize = 2;
const CLOTH_TARGET: u32 = 12;
const ANTISEPTIC_SHIFTS: usize = 2;
const ANTISEPTIC_TARGET: u32 = 12;
const ANTISEPTIC_LOAD: u32 = 1;
const SUBSTITUTE_REPAIR: u32 = 10;
const SUBSTITUTE_CONDITION_CAP: u32 = 80;
const ALGAE_QUALITY_PENALTY: u32 = 32;
const SUMMER_WATER_LOAD: u32 = 1;
const MATERIAL_STOCK_TARGET: u32 = 8;
const GRANARY_MAINTENANCE_THRESHOLD: u32 = 65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridReport {
    pub renewable: u32,
    pub backup: u32,
    pub requested: u32,
    pub delivered: u32,
    pub rated: u32,
    pub voltage: u32,
    pub frequency_centihz: u32,
}

impl GridReport {
    fn dispatch(renewable: u32, backup: u32, requested: u32, unused: u32) -> Self {
        let online = renewable + backup;
        let delivered = online - unused;
        let (voltage, frequency_centihz) = if online == 0 {
            (0, 0)
        } else {
            const INVERTER_DROOP_CENTIHZ: f64 = 75.0;
            const GENERATOR_DROOP_CENTIHZ: f64 = 200.0;
            let inverter = f64::from(renewable);
            let generator = f64::from(backup);
            let load = f64::from(delivered);
            // Parallel sources share a common frequency. Their available power per
            // centihertz of droop adds, while shed loads draw no current.
            let response = inverter / INVERTER_DROOP_CENTIHZ + generator / GENERATOR_DROOP_CENTIHZ;
            let frequency = 5_050.0 - load / response;
            // Approximate the source regulation and the 0.28-ohm feeder drop I×R
            // at unity power factor over the one-hour modeled dispatch window.
            let capacity = f64::from(online);
            let source_drop = (inverter * 7.0 + generator * 12.0) / capacity * (load / capacity);
            let load_watts = load * f64::from(Account::Power.measurement().0) * 100.0;
            let feeder_drop = load_watts / 230.0 * 0.28;
            (
                (236.0 - source_drop - feeder_drop).round() as u32,
                frequency.round() as u32,
            )
        };
        Self {
            renewable,
            backup,
            requested,
            delivered,
            rated: RENEWABLE_RATED + BACKUP_BATCH_LIMIT * POWER_PER_BATCH,
            voltage,
            frequency_centihz,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayReport {
    pub day: u32,
    pub weather: Weather,
    pub season: Season,
    pub population: usize,
    pub food_shortfall: u32,
    pub water_shortfall: u32,
    pub water_requested: u32,
    pub accepted_shifts: usize,
    pub completed_shifts: usize,
    pub unfilled_shifts: usize,
    pub medical_capacity: usize,
    pub treated: usize,
    pub kits_prepared: usize,
    pub grid: GridReport,
    pub backup_quota: u32,
    pub completed_crews: Vec<Vec<usize>>,
    pub waiting_for_care: usize,
    /// Zero-based range into the simulation's event log.
    pub events: Range<usize>,
}

const RISKY_FOOD_DECAY: u8 = 20;

fn food_illness_per_mille(decay: u8) -> u32 {
    if decay < RISKY_FOOD_DECAY {
        0
    } else {
        2 + u32::from(decay - RISKY_FOOD_DECAY) / 4
    }
}

fn water_illness_per_mille(quality: u32) -> u32 {
    60_u32.saturating_sub(quality) * 5
}

pub struct Simulation {
    world: World,
    social: Social,
    random: Random,
    weather_random: Random,
    illness_random: Random,
    clock: Random,
    rainy_days: u32,
    journal: Journal,
    pump_event: Option<usize>,
}

impl Simulation {
    pub fn broken_bridge(seed: u64) -> Self {
        let mut random = Random::new(seed);
        let world = World::broken_bridge(&mut random);
        Self {
            world,
            social: Social::default(),
            random,
            weather_random: Random::new(seed ^ 0x3deb_4af1_9d9b_7341),
            illness_random: Random::new(seed ^ 0x83b9_d2f1_4a76_c0e5),
            clock: Random::new(seed ^ 0xa424_d78a_5c6b_91f3),
            rainy_days: 0,
            journal: Journal::default(),
            pump_event: None,
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn events(&self) -> &[Event] {
        self.journal.events()
    }

    pub fn set_familiarity_enabled(&mut self, enabled: bool) {
        self.social.use_familiarity = enabled;
    }

    pub fn relationships(&self) -> impl Iterator<Item = (usize, usize, Relationship)> + '_ {
        self.social.relationships()
    }

    pub fn advance(&mut self) -> DayReport {
        let weather = if self.weather_random.below(4) == 0 {
            Weather::Rain
        } else {
            Weather::Clear
        };
        self.advance_with_weather(weather)
    }

    fn advance_with_weather(&mut self, weather: Weather) -> DayReport {
        let start = self.journal.events().len();
        self.world.day += 1;
        let season = season_of(self.world.day);
        let mut day = Day {
            world: &mut self.world,
            journal: &mut self.journal,
            pump_event: &mut self.pump_event,
            reserved: [false; RESIDENT_COUNT],
            worked: [None; RESIDENT_COUNT],
            completed_crews: Vec::new(),
            unfilled: 0,
            backup_used: 0,
            backup_quota: BACKUP_BATCH_LIMIT,
            backup_policy: None,
            social: &mut self.social,
        };

        day.journal.at(6, self.clock.below(30) as u16);
        let weather_event = day.note(Subject::Settlement, weather.name(), None);
        day.apply_backup_policy();
        if weather == Weather::Rain {
            self.rainy_days += 1;
            day.grow_wood(self.rainy_days, weather_event);
        }
        day.journal.at(7, self.clock.below(30) as u16);
        let opening_condition = day.world.pump_condition;
        let opening_wood = day.world.balance(Account::Wood);
        let water = day.schedule(Subject::Pump, Trade::Waterworks, WATER_SHIFTS, None);
        let fishing_priority = day
            .social
            .approved_fishing_priority
            .take()
            .map(|vote| day.note(Subject::Council, "fishing_priority_effective", Some(vote)));
        let fishing_first = fishing_priority.is_some()
            && day.eligible_workers(Subject::Farm, Trade::Food) < FIELD_SHIFTS + FISHING_SHIFTS;
        let (fields, fishers) = if fishing_first {
            let fishers = day.schedule(
                Subject::Fishery,
                Trade::Food,
                FISHING_SHIFTS,
                fishing_priority,
            );
            let fields = day.schedule(
                Subject::Farm,
                Trade::Food,
                FIELD_SHIFTS,
                Some(fishers.event),
            );
            (fields, fishers)
        } else {
            let fields = day.schedule(Subject::Farm, Trade::Food, FIELD_SHIFTS, None);
            let fishers = day.schedule(Subject::Fishery, Trade::Food, FISHING_SHIFTS, None);
            (fields, fishers)
        };
        let food_cause = if fishing_first {
            fields.event
        } else {
            fishers.event
        };
        let fields = fields.workers.as_slice();
        let fishers = fishers.workers.as_slice();
        let repair_slots = usize::from(opening_condition < 60);
        let repair = day.schedule(Subject::Pump, Trade::Mechanic, repair_slots, None);
        let granary_slots =
            usize::from(day.world.granary_condition < GRANARY_MAINTENANCE_THRESHOLD);
        let granary = day.schedule(Subject::Granary, Trade::Mechanic, granary_slots, None);
        let recovery = day.plan_recovery();
        let salvage = day.schedule(Subject::Salvage, Trade::General, recovery.len(), None);
        let fuel_slots = usize::from(
            weather == Weather::Clear && day.world.balance(Account::Fuel) < FUEL_YARD_TARGET,
        );
        let fuel_crew = day.schedule(Subject::FuelYard, Trade::General, fuel_slots, None);
        let cloth_slots = if day.world.balance(Account::Cloth) < CLOTH_TARGET {
            CLOTH_SHIFTS
        } else {
            0
        };
        let cloth = day.schedule(Subject::Farm, Trade::General, cloth_slots, None);
        let antiseptic_slots = if day.world.balance(Account::Antiseptic) < ANTISEPTIC_TARGET {
            ANTISEPTIC_SHIFTS
        } else {
            0
        };
        let antiseptic = day.schedule(Subject::Clinic, Trade::General, antiseptic_slots, None);

        let power = match weather {
            Weather::Clear => RENEWABLE_RATED,
            Weather::Rain => 5,
        };
        let farm_yield = season.farm_yield(weather);
        let fishing_yield = match weather {
            Weather::Clear => 6,
            Weather::Rain => 4,
        };
        day.journal.at(8, self.clock.below(30) as u16);
        let grid = day.post(
            Subject::Grid,
            "weather_generation",
            Some(weather_event),
            Account::Power,
            Flow::Produced,
            power,
        );
        let archive_power = day.post(
            Subject::Archive,
            "archive_load",
            Some(grid),
            Account::Power,
            Flow::Consumed,
            ARCHIVE_LOAD,
        );
        let grid = day.post(
            Subject::Settlement,
            "kitchen_load",
            Some(archive_power),
            Account::Power,
            Flow::Consumed,
            KITCHEN_LOAD,
        );
        let mut requested = ARCHIVE_LOAD + KITCHEN_LOAD;
        let water_power = if opening_condition <= 20 {
            let blocked = day.note(Subject::Pump, "pump_out_of_service", *day.pump_event);
            Err(blocked)
        } else if water.workers.is_empty() {
            let blocked = day.note(Subject::Pump, "no_water_crew", Some(water.event));
            Err(blocked)
        } else {
            requested += season.water_load();
            day.power_load("water_load", season.water_load(), grid)
        };
        let repair_power = if repair_slots == 0 {
            None
        } else if repair.workers.is_empty() {
            day.note(Subject::Pump, "no_repair_crew", Some(repair.event));
            None
        } else if day.world.balance(Account::Wood) == 0 {
            day.note(Subject::Pump, "insufficient_materials", Some(repair.event));
            None
        } else {
            let after_water = match water_power {
                Ok(event) | Err(event) => event,
            };
            requested += REPAIR_LOAD;
            day.power_load("repair_load", REPAIR_LOAD, after_water).ok()
        };
        let power_tail = repair_power.unwrap_or(match water_power {
            Ok(event) | Err(event) => event,
        });
        let antiseptic_load = antiseptic.workers.len() as u32 * ANTISEPTIC_LOAD;
        let antiseptic_power = if antiseptic_load == 0 {
            None
        } else {
            requested += antiseptic_load;
            day.power_load("antiseptic_load", antiseptic_load, power_tail)
                .ok()
        };

        day.journal.at(9, self.clock.below(30) as u16);
        let water_event = match water_power {
            Ok(power_event) => {
                let produced = water.workers.len() as u32 * 10;
                let before = day.world.water_quality();
                let runoff = if weather == Weather::Rain { 12 } else { 0 };
                let quality = 97 - (100 - opening_condition) / 3 - runoff - season.algae_penalty();
                if season.algae_penalty() > 0 {
                    day.note(Subject::Pump, "algae_bloom", Some(weather_event));
                }
                day.world.mix_treated_water(produced, quality);
                let output = day.post(
                    Subject::Pump,
                    "water_production",
                    Some(power_event),
                    Account::Water,
                    Flow::Produced,
                    produced,
                );
                let after = day.world.water_quality();
                if before != after {
                    day.journal.record(
                        day.world.day,
                        Subject::Pump,
                        "water_quality_changed",
                        Some(output),
                        Some((Change::WaterQuality, before, after)),
                    );
                }
                day.complete(&water.workers, output);
                output
            }
            Err(blocked) => blocked,
        };
        day.journal.at(10, self.clock.below(30) as u16);
        let allocation = day.journal.record(
            day.world.day,
            Subject::Cooperative,
            "food_crew_allocated",
            Some(food_cause),
            Some((Change::FoodCrew, fields.len() as u32, fishers.len() as u32)),
        );
        let harvest_cause = if fishing_first {
            allocation
        } else {
            weather_event
        };
        let drinking_requested = day.world.residents.iter().filter(|r| r.is_alive()).count() as u32;
        let irrigation_requested = match weather {
            Weather::Clear => fields.len() as u32,
            Weather::Rain => 0,
        };
        if irrigation_requested > 0 {
            let spare = day
                .world
                .balance(Account::Water)
                .saturating_sub(drinking_requested);
            let irrigation = spare.min(irrigation_requested);
            if irrigation > 0 {
                day.post(
                    Subject::Farm,
                    "seedbed_irrigation",
                    Some(water_event),
                    Account::Water,
                    Flow::Consumed,
                    irrigation,
                );
            }
            if irrigation < irrigation_requested {
                day.note(Subject::Farm, "irrigation_reserved", Some(water_event));
            }
        }

        day.journal.at(11, self.clock.below(30) as u16);
        let harvest = if fields.is_empty() {
            day.note(Subject::Farm, "no_food_crew", Some(allocation))
        } else {
            let harvest = day.post(
                Subject::Farm,
                "food_production",
                Some(harvest_cause),
                Account::Food,
                Flow::Produced,
                fields.len() as u32 * farm_yield,
            );
            day.complete(fields, harvest);
            harvest
        };
        let food_event = if fishers.is_empty() {
            harvest
        } else {
            let catch = day.post(
                Subject::Fishery,
                "fish_landed",
                Some(harvest_cause),
                Account::Food,
                Flow::Produced,
                fishers.len() as u32 * fishing_yield,
            );
            day.complete(fishers, catch);
            catch
        };
        day.journal.at(12, self.clock.below(30) as u16);
        if !cloth.workers.is_empty() {
            let amount = (cloth.workers.len() as u32).min(day.world.balance(Account::Food));
            if amount > 0 {
                let straw = day.post(
                    Subject::Farm,
                    "cloth_food_used",
                    Some(cloth.event),
                    Account::Food,
                    Flow::Consumed,
                    amount,
                );
                let woven = day.post(
                    Subject::Farm,
                    "cloth_production",
                    Some(straw),
                    Account::Cloth,
                    Flow::Produced,
                    amount,
                );
                day.complete(&cloth.workers[..amount as usize], woven);
            }
        }
        day.journal.at(13, self.clock.below(30) as u16);
        for (&worker, (plot, field, stock)) in salvage.workers.iter().zip(recovery) {
            let site = Subject::Plot(plot);
            let taken = day.post(
                site,
                "salvage_recovery",
                Some(salvage.event),
                field,
                Flow::TransferOut,
                1,
            );
            let stored = day.post(
                site,
                "salvage_recovery",
                Some(taken),
                stock,
                Flow::TransferIn,
                1,
            );
            day.complete(&[worker], stored);
        }
        day.journal.at(14, self.clock.below(30) as u16);
        if !fuel_crew.workers.is_empty() {
            let produced = FUEL_BATCH.min(FUEL_YARD_TARGET - day.world.balance(Account::Fuel));
            let fuel = day.post(
                Subject::FuelYard,
                "biomass_fuel_produced",
                Some(fuel_crew.event),
                Account::Fuel,
                Flow::Produced,
                produced,
            );
            day.complete(&fuel_crew.workers, fuel);
        }
        day.journal.at(15, self.clock.below(30) as u16);
        if water_power.is_ok() {
            day.condition("pump_wear", water_event, opening_condition - 2);
        }
        if let Some(power_event) = repair_power {
            let wood = day.post(
                Subject::Pump,
                "repair_materials",
                Some(power_event),
                Account::Wood,
                Flow::Consumed,
                1,
            );
            let (materials, restored) = if day.world.balance(Account::Steel) > 0 {
                let steel = day.post(
                    Subject::Pump,
                    "repair_materials",
                    Some(wood),
                    Account::Steel,
                    Flow::Consumed,
                    1,
                );
                (steel, (day.world.pump_condition + 16).min(100))
            } else {
                let substitute = day.note(Subject::Pump, "repair_substitute", Some(wood));
                (
                    substitute,
                    (day.world.pump_condition + SUBSTITUTE_REPAIR).min(SUBSTITUTE_CONDITION_CAP),
                )
            };
            let repaired = day.condition("pump_repaired", materials, restored);
            day.complete(&repair.workers, repaired);
        }

        day.journal.at(16, self.clock.below(30) as u16);
        let granary_event = day.maintain_granary(
            weather,
            weather_event,
            &granary,
            opening_wood > u32::from(repair_power.is_some()),
        );
        day.journal.at(17, self.clock.below(30) as u16);
        let (inside_decay, outside_decay) = match weather {
            Weather::Clear => (2, 25),
            Weather::Rain => (3, 35),
        };
        let (outside, spoiled) = day.world.age_food(inside_decay, outside_decay);
        let food_event = if outside > 0 {
            day.journal.record(
                day.world.day,
                Subject::Granary,
                "granary_overflow",
                Some(granary_event),
                Some((Change::FoodQuantity, 0, outside)),
            )
        } else {
            food_event
        };
        let food_event = if spoiled > 0 {
            let removed = day.post(
                Subject::Granary,
                "granary_spoilage",
                Some(food_event),
                Account::Food,
                Flow::Consumed,
                spoiled,
            );
            let fuel = (spoiled * Account::Food.measurement().0 / Account::Fuel.measurement().0)
                .min(FUEL_YARD_TARGET.saturating_sub(day.world.balance(Account::Fuel)));
            if fuel > 0 {
                day.post(
                    Subject::FuelYard,
                    "spoiled_food_to_fuel",
                    Some(removed),
                    Account::Fuel,
                    Flow::Produced,
                    fuel,
                );
            }
            removed
        } else {
            food_event
        };

        day.journal.at(18, self.clock.below(30) as u16);
        let water_risk = water_illness_per_mille(day.world.water_quality());
        let (fed, food_shortfall, food_rations, food_gap) = day.ration(Account::Food, food_event);
        let (hydrated, water_shortfall, water_rations, water_gap) =
            day.ration(Account::Water, water_event);
        day.journal.at(19, self.clock.below(30) as u16);
        if let Some(power_event) = antiseptic_power {
            let amount = (antiseptic.workers.len() as u32).min(day.world.balance(Account::Water));
            if amount > 0 {
                let water = day.post(
                    Subject::Clinic,
                    "antiseptic_water_used",
                    Some(power_event),
                    Account::Water,
                    Flow::Consumed,
                    amount,
                );
                let output = day.post(
                    Subject::Clinic,
                    "antiseptic_production",
                    Some(water),
                    Account::Antiseptic,
                    Flow::Produced,
                    amount,
                );
                day.complete(&antiseptic.workers[..amount as usize], output);
            }
        }
        day.journal.at(20, self.clock.below(30) as u16);
        for index in 0..RESIDENT_COUNT {
            if day.world.residents[index].is_alive() {
                let food_illness = fed[index].is_some_and(|decay| {
                    let chance = food_illness_per_mille(decay);
                    chance > 0 && self.illness_random.below(1_000) < chance
                });
                let drank = hydrated[index].is_some();
                let water_illness =
                    drank && water_risk > 0 && self.illness_random.below(1_000) < water_risk;
                let ate = fed[index].is_some();
                day.settle_resident(
                    index,
                    (ate, if ate { food_rations } else { food_gap }),
                    (drank, if drank { water_rations } else { water_gap }),
                    &mut self.random,
                    food_illness,
                    water_illness,
                );
            }
        }
        day.journal.at(21, self.clock.below(30) as u16);
        let (treated, waiting_for_care) = day.treat_patients();
        day.journal.at(22, self.clock.below(30) as u16);
        let (kits_prepared, clinic_water_requested) = day.prepare_kits(CLINIC_CAPACITY - treated);
        day.journal.at(23, 30);
        day.consider_backup_policy();
        if season == Season::Winter
            && season_of(day.world.day + 1) == Season::Winter
            && !fields.is_empty()
            && !fishers.is_empty()
        {
            day.consider_fishing_priority(farm_yield, fishing_yield, food_event);
        }
        day.journal.at(23, 59);
        let unused = day.world.balance(Account::Power);
        let backup = day.backup_used * POWER_PER_BATCH;
        let grid_report = GridReport::dispatch(power, backup, requested, unused);
        if unused > 0 {
            day.post(
                Subject::Grid,
                "power_expired",
                Some(grid),
                Account::Power,
                Flow::Consumed,
                unused,
            );
        }

        day.social.close_workday(&day.worked);
        DayReport {
            day: day.world.day,
            weather,
            season,
            population: day.world.residents.iter().filter(|r| r.is_alive()).count(),
            food_shortfall,
            water_shortfall,
            water_requested: drinking_requested
                + irrigation_requested
                + clinic_water_requested
                + antiseptic.workers.len() as u32,
            accepted_shifts: day.reserved.iter().filter(|&&reserved| reserved).count(),
            completed_shifts: day.worked.iter().flatten().count(),
            unfilled_shifts: day.unfilled,
            medical_capacity: CLINIC_CAPACITY,
            treated,
            kits_prepared,
            grid: grid_report,
            backup_quota: day.backup_quota,
            completed_crews: day.completed_crews,
            waiting_for_care,
            events: start..day.journal.events().len(),
        }
    }
}

const CLINIC_CAPACITY: usize = 3;

struct Crew {
    workers: Vec<usize>,
    event: usize,
}

struct Day<'a> {
    world: &'a mut World,
    journal: &'a mut Journal,
    pump_event: &'a mut Option<usize>,
    reserved: [bool; RESIDENT_COUNT],
    worked: [Option<usize>; RESIDENT_COUNT],
    completed_crews: Vec<Vec<usize>>,
    unfilled: usize,
    backup_used: u32,
    backup_quota: u32,
    backup_policy: Option<usize>,
    social: &'a mut Social,
}

impl Day<'_> {
    fn note(&mut self, subject: Subject, reason: &'static str, cause: Option<usize>) -> usize {
        self.journal
            .record(self.world.day, subject, reason, cause, None)
    }

    fn post(
        &mut self,
        subject: Subject,
        reason: &'static str,
        cause: Option<usize>,
        account: Account,
        flow: Flow,
        amount: u32,
    ) -> usize {
        self.journal
            .post(self.world, subject, reason, cause, (account, flow, amount))
    }

    fn apply_backup_policy(&mut self) {
        let Some(approval) = self.social.approved_limit.take() else {
            return;
        };
        let water = self.world.balance(Account::Water);
        let reason = if water <= WATER_RESERVE {
            "backup_reserve_override"
        } else {
            self.backup_quota = 0;
            "backup_limit_effective"
        };
        let event = self.journal.record(
            self.world.day,
            Subject::Council,
            reason,
            Some(approval),
            Some((Change::BackupQuota, BACKUP_BATCH_LIMIT, self.backup_quota)),
        );
        if self.backup_quota < BACKUP_BATCH_LIMIT {
            self.backup_policy = Some(event);
        }
    }

    fn consider_backup_policy(&mut self) {
        let fuel = self.world.balance(Account::Fuel);
        let water = self.world.balance(Account::Water);
        if !(FUEL_BATCH * BACKUP_BATCH_LIMIT..=FUEL_REVIEW).contains(&fuel)
            || water <= WATER_RESERVE
        {
            return;
        }
        let Some(exchange) = EXCHANGE
            .into_iter()
            .find(|&id| can_attend(&self.world.residents[id - 1]))
        else {
            return;
        };
        let Some(proposer) = COOPERATIVE
            .into_iter()
            .find(|&id| can_attend(&self.world.residents[id - 1]))
        else {
            return;
        };
        let signed = self.note(Subject::Resident(exchange), "exchange_signed_stock", None);
        let stock = self.journal.record(
            self.world.day,
            Subject::Exchange,
            "backup_stock_review",
            Some(signed),
            Some((Change::ProposalStock, fuel, water)),
        );
        let pump = self.world.pump_condition;
        let quality = self.world.water_quality();
        let reviewed = self.journal.record(
            self.world.day,
            Subject::Cooperative,
            "backup_service_review",
            Some(stock),
            Some((Change::ProposalCondition, pump, quality)),
        );
        let proposed = self.note(
            Subject::Resident(proposer),
            "backup_limit_proposed",
            Some(reviewed),
        );
        self.social.approved_limit = self.council_vote(
            proposed,
            [
                "backup_vote_yes",
                "backup_vote_no",
                "backup_vote_passed",
                "backup_vote_rejected",
                "backup_vote_no_quorum",
            ],
            |resident| supports_limit(resident, pump, quality),
        );
    }

    fn consider_fishing_priority(&mut self, farm_yield: u32, fishing_yield: u32, harvest: usize) {
        let food = self.world.balance(Account::Food);
        let population = self
            .world
            .residents
            .iter()
            .filter(|resident| resident.is_alive())
            .count() as u32;
        if fishing_yield < farm_yield || (fishing_yield == farm_yield && food < 3 * population) {
            return;
        }
        let Some(proposer) = COOPERATIVE
            .into_iter()
            .find(|&id| can_attend(&self.world.residents[id - 1]))
        else {
            return;
        };
        let stock = self.journal.record(
            self.world.day,
            Subject::Granary,
            "food_vote_stock_review",
            Some(harvest),
            Some((Change::FoodQuantity, 0, food)),
        );
        let rates = self.journal.record(
            self.world.day,
            Subject::Cooperative,
            "food_vote_harvest_review",
            Some(stock),
            Some((Change::HarvestRates, farm_yield, fishing_yield)),
        );
        let proposed = self.note(
            Subject::Resident(proposer),
            "fishing_priority_proposed",
            Some(rates),
        );
        self.social.approved_fishing_priority = self.council_vote(
            proposed,
            [
                "food_vote_yes",
                "food_vote_no",
                "food_vote_passed",
                "food_vote_rejected",
                "food_vote_no_quorum",
            ],
            |resident| {
                fishing_yield > farm_yield || matches!(resident.trade, Trade::Food | Trade::General)
            },
        );
    }

    fn council_vote(
        &mut self,
        proposed: usize,
        [support, oppose, passed, rejected, no_quorum]: [&'static str; 5],
        supports: impl Fn(&Resident) -> bool,
    ) -> Option<usize> {
        let mut yes = 0;
        let mut no = 0;
        let mut latest = proposed;
        for id in COUNCIL {
            let resident = &self.world.residents[id - 1];
            if !can_attend(resident) {
                continue;
            }
            let reason = if supports(resident) {
                yes += 1;
                support
            } else {
                no += 1;
                oppose
            };
            latest = self.note(Subject::Resident(id), reason, Some(latest));
        }
        let eligible = COUNCIL
            .iter()
            .filter(|&&id| self.world.residents[id - 1].is_alive())
            .count() as u32;
        let reason = if yes + no <= eligible / 2 {
            no_quorum
        } else if yes > no {
            passed
        } else {
            rejected
        };
        let outcome = self.journal.record(
            self.world.day,
            Subject::Council,
            reason,
            Some(latest),
            Some((Change::VoteTally, yes, no)),
        );
        (reason == passed).then_some(outcome)
    }

    fn grow_wood(&mut self, rainy_days: u32, weather_event: usize) {
        for index in 0..self.world.plots().len() {
            let plot = self.world.plots()[index];
            if plot.wood >= plot.wood_capacity
                || !(rainy_days + u32::from(plot.col)).is_multiple_of(4)
            {
                continue;
            }
            self.post(
                Subject::Plot(index),
                "wood_regrowth",
                Some(weather_event),
                Account::FieldWood,
                Flow::Produced,
                2,
            );
        }
    }

    fn plan_recovery(&self) -> Vec<(usize, Account, Account)> {
        let mut jobs = Vec::new();
        for (field, stock) in [
            (Account::FieldSteel, Account::Steel),
            (Account::FieldWood, Account::Wood),
        ] {
            let mut needed = MATERIAL_STOCK_TARGET.saturating_sub(self.world.balance(stock));
            for (index, plot) in self
                .world
                .plots()
                .iter()
                .enumerate()
                .filter(|(_, plot)| plot.accessible)
            {
                let site_balance = if field == Account::FieldSteel {
                    plot.steel
                } else {
                    plot.wood
                };
                let take = site_balance
                    .min(needed)
                    .min((RECOVERY_SHIFTS - jobs.len()) as u32);
                jobs.extend((0..take).map(|_| (index, field, stock)));
                needed -= take;
                if needed == 0 || jobs.len() == RECOVERY_SHIFTS {
                    break;
                }
            }
        }
        jobs
    }

    fn maintain_granary(
        &mut self,
        weather: Weather,
        weather_event: usize,
        crew: &Crew,
        has_wood: bool,
    ) -> usize {
        let before = self.world.granary_condition;
        let wear = if weather == Weather::Rain { 2 } else { 1 };
        let after = before.saturating_sub(wear);
        self.world.granary_condition = after;
        let wear_event = self.journal.record(
            self.world.day,
            Subject::Granary,
            "granary_wear",
            Some(weather_event),
            Some((Change::Condition, before, after)),
        );
        if crew.workers.is_empty() {
            return wear_event;
        }
        if !has_wood {
            self.note(Subject::Granary, "granary_no_wood", Some(crew.event));
            return wear_event;
        }
        let wood = self.post(
            Subject::Granary,
            "granary_wood_used",
            Some(crew.event),
            Account::Wood,
            Flow::Consumed,
            1,
        );
        let restored = (after + 20).min(100);
        self.world.granary_condition = restored;
        let repair = self.journal.record(
            self.world.day,
            Subject::Granary,
            "granary_repaired",
            Some(wood),
            Some((Change::Condition, after, restored)),
        );
        self.complete(&crew.workers, repair);
        repair
    }

    fn schedule(
        &mut self,
        site: Subject,
        trade: Trade,
        slots: usize,
        cause: Option<usize>,
    ) -> Crew {
        let reason = match (trade, site) {
            (Trade::Waterworks, _) => "water_shifts",
            (Trade::Food, _) => "food_shifts",
            (Trade::Mechanic, _) => "repair_shifts",
            (Trade::General, Subject::FuelYard) => "fuel_shifts",
            (Trade::General, Subject::Farm) => "cloth_shifts",
            (Trade::General, Subject::Clinic) => "antiseptic_shifts",
            (Trade::General, _) => "salvage_shifts",
        };
        if slots == 0 {
            return Crew {
                workers: Vec::new(),
                event: self.note(site, "no_shifts_required", None),
            };
        }
        let request = self.note(site, reason, cause);
        let mut workers = Vec::new();
        let mut last_refusal = None;
        let pools: &[_] = if trade == Trade::General {
            &[Trade::General]
        } else {
            &[trade, Trade::General]
        };
        for pool in pools {
            let vacancies = slots - workers.len();
            if vacancies == 0 {
                break;
            }
            let mut eligible = Vec::new();
            for index in self.candidates(*pool) {
                if let Some(reason) = self.refusal(site, index) {
                    let who = Subject::Resident(self.world.residents[index].id);
                    last_refusal = Some(self.note(who, reason, Some(request)));
                } else {
                    eligible.push(index);
                }
            }
            let day = self.world.day;
            let fairness_key = |index: usize| {
                (
                    self.social.workload(index),
                    (index + RESIDENT_COUNT - (day as usize - 1) % RESIDENT_COUNT) % RESIDENT_COUNT,
                )
            };
            eligible.sort_by_key(|&index| fairness_key(index));
            let baseline: Vec<_> = eligible.iter().take(vacancies).copied().collect();
            let already_selected = workers.len();
            while workers.len() < slots && !eligible.is_empty() {
                let position = eligible
                    .iter()
                    .enumerate()
                    .min_by_key(|&(_, &index)| {
                        let (workload, rotation) = fairness_key(index);
                        let affinity: u32 = workers
                            .iter()
                            .map(|&partner| self.social.affinity(index, partner, day))
                            .sum();
                        (workload, std::cmp::Reverse(affinity), rotation)
                    })
                    .expect("eligible candidates are available")
                    .0;
                workers.push(eligible.remove(position));
            }
            let chosen = &workers[already_selected..];
            let mut displaced = baseline.iter().filter(|index| !chosen.contains(index));
            for &index in chosen {
                self.reserved[index] = true;
                let id = self.world.residents[index].id;
                let priority = if baseline.contains(&index) {
                    None
                } else {
                    let replaced = self.world.residents[*displaced
                        .next()
                        .expect("a replacement has a displaced candidate")]
                    .id;
                    Some(self.journal.record_pair(
                        day,
                        (id, replaced),
                        "crew_familiarity_priority",
                        Some(request),
                        (Change::CrewPriority, 0, 1),
                    ))
                };
                self.note(
                    Subject::Resident(id),
                    "shift_accepted",
                    priority.or(Some(request)),
                );
            }
        }
        let missing = slots - workers.len();
        let event = if missing == 0 {
            request
        } else {
            self.unfilled += missing;
            self.journal.record(
                self.world.day,
                site,
                "shifts_unfilled",
                last_refusal.or(Some(request)),
                Some((Change::Unfilled, 0, missing as u32)),
            )
        };
        Crew { workers, event }
    }

    fn candidates(&self, trade: Trade) -> Vec<usize> {
        (0..RESIDENT_COUNT)
            .filter(|&index| {
                let resident = &self.world.residents[index];
                !self.reserved[index] && resident.is_alive() && resident.trade == trade
            })
            .collect()
    }

    fn eligible_workers(&self, site: Subject, trade: Trade) -> usize {
        [trade, Trade::General]
            .into_iter()
            .flat_map(|pool| self.candidates(pool))
            .filter(|&index| self.refusal(site, index).is_none())
            .count()
    }

    fn refusal(&self, site: Subject, index: usize) -> Option<&'static str> {
        let resident = &self.world.residents[index];
        if resident.health <= 40 {
            Some("shift_refused_health")
        } else if resident.fatigue >= 70 {
            Some("shift_refused_fatigue")
        } else if site == Subject::Pump
            && self.world.pump_condition <= 30
            && (resident.health <= 70 || resident.fatigue >= 45)
        {
            Some("shift_refused_risk")
        } else {
            None
        }
    }

    /// Returns the allocation event or the event explaining why the load was denied.
    fn power_load(
        &mut self,
        reason: &'static str,
        load: u32,
        previous: usize,
    ) -> Result<usize, usize> {
        let available = self.world.balance(Account::Power);
        let batches = load.saturating_sub(available).div_ceil(POWER_PER_BATCH);
        let fuel = batches * FUEL_BATCH;
        let mut cause = previous;
        if fuel > 0 {
            let fuel_available = self
                .world
                .balance(Account::Fuel)
                .min((self.backup_quota - self.backup_used) * FUEL_BATCH);
            if fuel > fuel_available {
                if let Some(policy) = self.backup_policy
                    && batches <= BACKUP_BATCH_LIMIT - self.backup_used
                    && self.world.balance(Account::Fuel) >= fuel
                {
                    cause = self.note(Subject::Grid, "backup_blocked_by_vote", Some(policy));
                }
                let denied = match reason {
                    "water_load" => "water_power_denied",
                    "repair_load" => "repair_power_denied",
                    _ => "antiseptic_power_denied",
                };
                return Err(self.journal.record(
                    self.world.day,
                    Subject::Pump,
                    denied,
                    Some(cause),
                    Some((Change::BackupFuel, fuel_available, fuel)),
                ));
            }
            self.backup_used += batches;
            let consumed = self.post(
                Subject::Grid,
                "backup_fuel",
                Some(previous),
                Account::Fuel,
                Flow::Consumed,
                fuel,
            );
            cause = self.post(
                Subject::Grid,
                "backup_generation",
                Some(consumed),
                Account::Power,
                Flow::Produced,
                batches * POWER_PER_BATCH,
            );
        }
        Ok(self.post(
            Subject::Pump,
            reason,
            Some(cause),
            Account::Power,
            Flow::Consumed,
            load,
        ))
    }

    fn complete(&mut self, workers: &[usize], cause: usize) {
        for &index in workers {
            let resident = Subject::Resident(self.world.residents[index].id);
            let event = self.note(resident, "shift_completed", Some(cause));
            self.worked[index] = Some(event);
        }
        if workers.len() > 1 {
            self.completed_crews.push(
                workers
                    .iter()
                    .map(|&index| self.world.residents[index].id)
                    .collect(),
            );
        }
        for (position, &left) in workers.iter().enumerate() {
            for &right in &workers[position + 1..] {
                if self.social.work_together(left, right, self.world.day) == 1 {
                    self.journal.record_pair(
                        self.world.day,
                        (
                            self.world.residents[left].id,
                            self.world.residents[right].id,
                        ),
                        "coworkers_met",
                        self.worked[left],
                        (Change::Familiarity, 0, 1),
                    );
                }
            }
        }
    }

    fn condition(&mut self, reason: &'static str, cause: usize, after: u32) -> usize {
        let before = std::mem::replace(&mut self.world.pump_condition, after);
        let event = self.journal.record(
            self.world.day,
            Subject::Pump,
            reason,
            Some(cause),
            Some((Change::Condition, before, after)),
        );
        *self.pump_event = Some(event);
        event
    }

    fn ration(
        &mut self,
        account: Account,
        supply: usize,
    ) -> ([Option<u8>; RESIDENT_COUNT], u32, usize, usize) {
        let mut served = [None; RESIDENT_COUNT];
        let stock = self.world.balance(account);
        let mut remaining = stock;
        let mut shortfall = 0;
        let start = (self.world.day as usize - 1) % RESIDENT_COUNT;
        for offset in 0..RESIDENT_COUNT {
            let index = (start + offset) % RESIDENT_COUNT;
            if self.world.residents[index].is_alive() {
                if remaining > 0 {
                    let decay = if account == Account::Food {
                        self.world.ration_decay((stock - remaining) as usize)
                    } else {
                        0
                    };
                    served[index] = Some(decay);
                    remaining -= 1;
                } else {
                    shortfall += 1;
                }
            }
        }
        let delivered = stock - remaining;
        let served_event = if delivered > 0 {
            self.post(
                Subject::Settlement,
                "rations_distributed",
                Some(supply),
                account,
                Flow::Consumed,
                delivered,
            )
        } else {
            supply
        };
        let risky = served
            .iter()
            .flatten()
            .filter(|&&decay| decay >= RISKY_FOOD_DECAY)
            .count() as u32;
        let served_event = if risky > 0 {
            self.journal.record(
                self.world.day,
                Subject::Granary,
                "risky_food_rations",
                Some(served_event),
                Some((Change::FoodQuantity, 0, risky)),
            )
        } else {
            served_event
        };
        let reason = match account {
            Account::Food => "food_shortfall",
            _ => "water_shortfall",
        };
        let event = if shortfall > 0 {
            self.note(Subject::Settlement, reason, Some(served_event))
        } else {
            served_event
        };
        (served, shortfall, served_event, event)
    }

    fn treat_patients(&mut self) -> (usize, usize) {
        let mut queue: Vec<_> = self
            .world
            .residents
            .iter()
            .enumerate()
            .filter(|(_, resident)| resident.is_alive() && resident.health <= 85)
            .map(|(index, resident)| (resident.health, resident.id, index))
            .collect();
        queue.sort_unstable();
        let treated = queue
            .len()
            .min(CLINIC_CAPACITY)
            .min(self.world.balance(Account::Medicine) as usize);
        for &(before, id, index) in &queue[..treated] {
            let patient = Subject::Resident(id);
            let cause = self
                .journal
                .events()
                .iter()
                .rev()
                .find(|event| {
                    event.subject == patient && matches!(event.change, Some((Change::Health, _, _)))
                })
                .map(|event| event.id);
            let supplies = self.post(
                Subject::Clinic,
                "clinic_supplies",
                cause,
                Account::Medicine,
                Flow::Consumed,
                1,
            );
            let after = before + 10;
            self.world.residents[index].health = after;
            self.journal.record(
                self.world.day,
                patient,
                "medical_treatment",
                Some(supplies),
                Some((Change::Health, before, after)),
            );
        }
        let waiting = queue.len() - treated;
        if waiting > 0 {
            self.note(Subject::Clinic, "clinic_waiting", None);
        }
        (treated, waiting)
    }

    fn prepare_kits(&mut self, remaining_staff_slots: usize) -> (usize, u32) {
        const RESTOCK_LEVEL: u32 = 8;
        let requested = RESTOCK_LEVEL
            .saturating_sub(self.world.balance(Account::Medicine))
            .min(remaining_staff_slots.min(2) as u32)
            .min(self.world.balance(Account::Cloth))
            .min(self.world.balance(Account::Antiseptic));
        let amount = requested.min(self.world.balance(Account::Water));
        if amount == 0 {
            return (0, requested);
        }
        let mut cause = Some(self.note(Subject::Clinic, "kit_preparation", None));
        for (account, reason) in [
            (Account::Cloth, "kit_cloth_used"),
            (Account::Antiseptic, "kit_antiseptic_used"),
            (Account::Water, "kit_water_used"),
        ] {
            cause = Some(self.post(
                Subject::Clinic,
                reason,
                cause,
                account,
                Flow::Consumed,
                amount,
            ));
        }
        self.post(
            Subject::Clinic,
            "kits_produced",
            cause,
            Account::Medicine,
            Flow::Produced,
            amount,
        );
        (amount as usize, requested)
    }

    fn settle_resident(
        &mut self,
        index: usize,
        (fed, food_cause): (bool, usize),
        (hydrated, water_cause): (bool, usize),
        random: &mut Random,
        food_illness: bool,
        water_illness: bool,
    ) {
        let shift = self.worked[index];
        let resident = &mut self.world.residents[index];
        let who = Subject::Resident(resident.id);
        let day = self.world.day;
        let journal = &mut self.journal;
        let mut change = |kind, value: &mut u32, delta, reason, cause| {
            let before = *value;
            *value = before.saturating_add_signed(delta).min(100);
            if before == *value {
                cause
            } else {
                Some(journal.record(day, who, reason, cause, Some((kind, before, *value))))
            }
        };
        let injury_risk = 1 + resident.fatigue / 10;
        change(
            Change::Fatigue,
            &mut resident.fatigue,
            if shift.is_some() { 12 } else { -20 },
            if shift.is_some() { "work" } else { "rest" },
            shift,
        );
        let hunger = change(
            Change::Hunger,
            &mut resident.hunger,
            if fed { -12 } else { 20 },
            if fed { "food_received" } else { "food_missed" },
            Some(food_cause),
        );
        let thirst = change(
            Change::Thirst,
            &mut resident.thirst,
            if hydrated { -20 } else { 35 },
            if hydrated {
                "water_received"
            } else {
                "water_missed"
            },
            Some(water_cause),
        );
        let mut death_cause = None;
        if shift.is_some() && random.below(100) < injury_risk {
            death_cause = change(
                Change::Health,
                &mut resident.health,
                -15,
                "work_injury",
                shift,
            );
        }
        if resident.is_alive() && food_illness {
            death_cause = change(
                Change::Health,
                &mut resident.health,
                -10,
                "foodborne_illness",
                Some(food_cause),
            );
        }
        if resident.is_alive() && water_illness {
            death_cause = change(
                Change::Health,
                &mut resident.health,
                -15,
                "waterborne_illness",
                Some(water_cause),
            );
        }
        if resident.is_alive() && resident.hunger >= 40 {
            death_cause = change(
                Change::Health,
                &mut resident.health,
                -4,
                "hunger_damage",
                hunger,
            );
        }
        if resident.is_alive() && resident.thirst >= 35 {
            death_cause = change(
                Change::Health,
                &mut resident.health,
                -8,
                "thirst_damage",
                thirst,
            );
        }
        if !resident.is_alive() {
            journal.record(day, who, "resident_died", death_cause, None);
        } else if fed && hydrated && shift.is_none() && !food_illness && !water_illness {
            change(
                Change::Health,
                &mut resident.health,
                5,
                "rest_recovery",
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::totals;

    /// Blocks the harvest and the straw that feeds weaving so kit tests can
    /// control the cloth stock directly. General workers are included because
    /// they backfill farm shifts and staff the loom.
    fn stop_food_production(simulation: &mut Simulation) {
        for resident in &mut simulation.world.residents {
            if matches!(resident.trade, Trade::Food | Trade::General) {
                resident.fatigue = 70;
            }
        }
        simulation.world.set_balance(Account::Food, 0);
    }

    #[test]
    fn overflow_ages_the_food_and_rations_use_the_most_deteriorated_portions_first() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.set_balance(Account::Food, 710);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events];
        let overflow = events
            .iter()
            .find(|event| event.reason == "granary_overflow")
            .unwrap();
        assert_eq!(overflow.change, Some((Change::FoodQuantity, 0, 114)));
        assert!(
            events
                .iter()
                .all(|event| event.reason != "granary_spoilage")
        );
        let risky = events
            .iter()
            .find(|event| event.reason == "risky_food_rations")
            .unwrap();
        assert_eq!(risky.change, Some((Change::FoodQuantity, 0, 100)));
        assert_eq!(simulation.world.balance(Account::Food), 730);
        assert_eq!(
            simulation.world.food_quality(),
            [[716, 0, 0, 0, 0], [0, 14, 0, 0, 0]]
        );
        assert_eq!(report.food_shortfall, 0);
    }

    #[test]
    fn inedible_food_is_logged_and_discarded_before_rations() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.set_balance(Account::Food, 110);
        simulation.world.age_food(98, 98);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events];
        let loss = events
            .iter()
            .find(|event| event.reason == "granary_spoilage")
            .unwrap();
        assert!(matches!(
            loss.change,
            Some((
                Change::Resource {
                    account: Account::Food,
                    flow: Flow::Consumed
                },
                230,
                120
            ))
        ));
        let rations = events
            .iter()
            .find(|event| event.reason == "rations_distributed" && event.cause == Some(loss.id))
            .unwrap();
        assert!(loss.minute < rations.minute);
        assert_eq!(report.food_shortfall, 0);
        assert_eq!(simulation.world.balance(Account::Food), 20);
    }

    #[test]
    fn spoiled_food_becomes_fuel_for_the_next_dispatch() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.set_balance(Account::Fuel, 0);
        simulation.world.set_balance(Account::Food, 5);
        simulation.world.age_food(98, 98);
        let first = simulation.advance_with_weather(Weather::Rain);
        let events = &simulation.events()[first.events];
        let loss = events
            .iter()
            .find(|event| event.reason == "granary_spoilage")
            .unwrap();
        let reclaimed = events
            .iter()
            .find(|event| event.reason == "spoiled_food_to_fuel")
            .unwrap();
        assert_eq!(reclaimed.cause, Some(loss.id));
        let lost_food = totals(events, Account::Food).consumed - 80;
        let new_fuel = totals(events, Account::Fuel).produced;
        assert_eq!(lost_food, 5);
        assert_eq!(
            lost_food * u64::from(Account::Food.measurement().0),
            new_fuel * u64::from(Account::Fuel.measurement().0)
        );
        assert_eq!(simulation.world.balance(Account::Fuel), 15);

        let second = simulation.advance_with_weather(Weather::Rain);
        let events = &simulation.events()[second.events];
        assert_eq!(
            totals(events, Account::Fuel).consumed,
            u64::from(FUEL_BATCH)
        );
        assert_eq!(second.grid.backup, POWER_PER_BATCH);
        assert_eq!(simulation.world.balance(Account::Fuel), 5);
    }

    #[test]
    fn spoiled_food_cannot_overfill_the_fuel_yard() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation
            .world
            .set_balance(Account::Fuel, FUEL_YARD_TARGET);
        simulation.world.set_balance(Account::Food, 5);
        simulation.world.age_food(98, 98);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events.clone()];
        assert!(
            events
                .iter()
                .any(|event| event.reason == "granary_spoilage")
        );
        assert_eq!(simulation.world().balance(Account::Fuel), FUEL_YARD_TARGET);
    }

    #[test]
    fn heavily_aged_food_can_cause_illness_with_a_ration_cause() {
        assert_eq!(food_illness_per_mille(9), 0);
        assert!(food_illness_per_mille(80) > food_illness_per_mille(25));
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.age_food(80, 80);
        let illness_seed = (0..1_000)
            .find(|&seed| Random::new(seed).below(1_000) < food_illness_per_mille(82))
            .unwrap();
        simulation.illness_random = Random::new(illness_seed);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events];
        let illness = events
            .iter()
            .find(|event| event.reason == "foodborne_illness")
            .unwrap();
        let cause = &simulation.events()[illness.cause.unwrap() - 1];
        assert_eq!(cause.reason, "risky_food_rations");
        assert!(
            matches!(illness.change, Some((Change::Health, before, after)) if before - after == 10)
        );
    }

    #[test]
    fn granary_maintenance_spends_wood_and_restores_storage_capacity() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.granary_condition = 64;
        let report = simulation.advance_with_weather(Weather::Rain);
        let events = &simulation.events()[report.events];
        assert_eq!(simulation.world.granary_condition, 82);
        assert_eq!(simulation.world.granary_capacity(), 728);
        assert_eq!(totals(events, Account::Wood).consumed, 1);
        assert!(
            events
                .iter()
                .any(|event| event.reason == "granary_repaired")
        );

        simulation.world.granary_condition = 64;
        simulation.world.set_balance(Account::Wood, 0);
        let report = simulation.advance_with_weather(Weather::Rain);
        let events = &simulation.events()[report.events];
        assert_eq!(simulation.world.granary_condition, 62);
        assert_eq!(totals(events, Account::Wood).consumed, 0);
        assert!(events.iter().any(|event| event.reason == "granary_no_wood"));
    }

    #[test]
    fn rain_regrows_recoverable_wood_only_at_woodland_plots() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.pump_condition = 100;
        let opening = simulation.world().plots().to_vec();
        for _ in 0..4 {
            simulation.advance_with_weather(Weather::Clear);
        }
        assert_eq!(simulation.world().plots(), opening.as_slice());
        for _ in 0..4 {
            simulation.advance_with_weather(Weather::Rain);
        }
        for (old, current) in opening.iter().zip(simulation.world().plots()) {
            assert_eq!(old.steel, current.steel);
            if old.wood_capacity > 0 {
                assert_eq!(old.wood + 2, current.wood);
                assert!(current.wood <= current.wood_capacity);
            } else {
                assert_eq!(old.wood, current.wood);
            }
        }
        assert_eq!(totals(simulation.events(), Account::FieldWood).produced, 8);
        assert_eq!(totals(simulation.events(), Account::Wood).produced, 0);
    }

    #[test]
    fn recovery_excludes_sealed_plots_and_transfers_matching_material() {
        let mut simulation = Simulation::broken_bridge(42);
        let opening = simulation.world().plots().to_vec();
        simulation.world.set_balance(Account::Wood, 7);
        simulation.world.set_balance(Account::Steel, 7);
        let recovered = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[recovered.events];
        assert_eq!(simulation.world().plots()[0].wood, opening[0].wood - 1);
        assert_eq!(simulation.world().plots()[0].steel, opening[0].steel - 1);
        assert_eq!(totals(events, Account::Wood).received, 1);
        assert_eq!(totals(events, Account::Steel).received, 1);

        for (index, plot) in simulation.world().plots().to_vec().iter().enumerate() {
            if plot.accessible {
                simulation
                    .world
                    .change_plot(index, Account::FieldWood, -i64::from(plot.wood));
                simulation
                    .world
                    .change_plot(index, Account::FieldSteel, -i64::from(plot.steel));
            }
        }
        simulation.world.set_balance(Account::Wood, 0);
        simulation.world.set_balance(Account::Steel, 0);
        let blocked = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[blocked.events];
        assert_eq!(totals(events, Account::Wood).received, 0);
        assert_eq!(totals(events, Account::Steel).received, 0);
        assert!(simulation.world().balance(Account::FieldSteel) > 0);
    }

    #[test]
    fn initializes_seeded_broken_bridge() {
        let first = Simulation::broken_bridge(42);
        let repeat = Simulation::broken_bridge(42);
        assert_eq!(first.world(), repeat.world());
        assert_ne!(first.world(), Simulation::broken_bridge(43).world());
        let world = first.world();
        assert_eq!(world.residents.len(), 100);
        let names = world
            .residents
            .iter()
            .map(|r| r.name)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), world.residents.len());
        assert_eq!(world.pump_condition, 70);
        for (account, balance) in
            Account::ALL
                .into_iter()
                .zip([400, 400, FUEL_YARD_TARGET, 8, 8, 38, 76, 0, 24, 24, 24])
        {
            assert_eq!(world.balance(account), balance);
        }
        for (trade, count) in [
            (Trade::Waterworks, 20),
            (Trade::Food, 33),
            (Trade::Mechanic, 13),
            (Trade::General, 34),
        ] {
            assert_eq!(
                world.residents.iter().filter(|r| r.trade == trade).count(),
                count
            );
        }
    }

    #[test]
    fn hundred_days_reproduce_and_reconcile_every_account() {
        for seed in [0, 42, 43, u64::MAX] {
            let mut first = Simulation::broken_bridge(seed);
            let mut repeat = Simulation::broken_bridge(seed);
            for date in 1..=100 {
                let opening = first.world().clone();
                let report = first.advance();
                assert_eq!(report, repeat.advance());
                assert_eq!(report.day, date);
                let events = &first.events()[report.events.clone()];
                assert_eq!(events, &repeat.events()[report.events.clone()]);
                for (index, event) in report.events.clone().zip(events) {
                    assert_eq!(event.id, index + 1);
                    assert_eq!(event.day, date);
                    assert!(event.minute < 24 * 60);
                    if let Some(cause) = event.cause {
                        assert!(cause > 0 && cause < event.id);
                        let previous = &first.events()[cause - 1];
                        assert!((previous.day, previous.minute) <= (date, event.minute));
                    }
                }
                assert!(
                    events
                        .windows(2)
                        .all(|pair| pair[0].minute <= pair[1].minute)
                );
                for account in Account::ALL {
                    let mut balance = opening.balance(account);
                    for event in events {
                        if let Some((
                            Change::Resource {
                                account: posted,
                                flow,
                            },
                            before,
                            after,
                        )) = event.change
                            && posted == account
                        {
                            assert_eq!(before, balance);
                            match flow {
                                Flow::Produced | Flow::TransferIn => assert!(after > before),
                                Flow::Consumed | Flow::TransferOut => assert!(after < before),
                            }
                            balance = after;
                        }
                    }
                    assert_eq!(balance, first.world().balance(account));
                }
                assert_eq!(
                    totals(events, Account::Wood).received,
                    totals(events, Account::FieldWood).sent
                );
                assert_eq!(
                    totals(events, Account::Steel).received,
                    totals(events, Account::FieldSteel).sent
                );
                assert_eq!(first.world().balance(Account::Power), 0);
                assert!(first.world().pump_condition <= 100);
                for resident in &first.world().residents {
                    assert!(resident.health <= 100);
                    assert!(resident.hunger <= 100);
                    assert!(resident.thirst <= 100);
                    assert!(resident.fatigue <= 100);
                    let shifts = events.iter().filter(|event| {
                        event.subject == Subject::Resident(resident.id)
                            && event.reason == "shift_completed"
                    });
                    assert!(shifts.count() <= 1);
                }
            }
            assert_eq!(first.world(), repeat.world());
        }
    }

    #[test]
    fn food_sources_and_uses_are_timed_and_separately_posted() {
        let mut simulation = Simulation::broken_bridge(42);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events];
        for (subject, account, flow, reason, minute, amount) in [
            (
                Subject::Farm,
                Account::Water,
                Flow::Consumed,
                "seedbed_irrigation",
                600,
                13,
            ),
            (
                Subject::Farm,
                Account::Food,
                Flow::Produced,
                "food_production",
                660,
                78,
            ),
            (
                Subject::Fishery,
                Account::Food,
                Flow::Produced,
                "fish_landed",
                660,
                42,
            ),
            (
                Subject::Settlement,
                Account::Food,
                Flow::Consumed,
                "rations_distributed",
                1080,
                100,
            ),
            (
                Subject::Archive,
                Account::Power,
                Flow::Consumed,
                "archive_load",
                480,
                1,
            ),
            (
                Subject::Settlement,
                Account::Power,
                Flow::Consumed,
                "kitchen_load",
                480,
                1,
            ),
        ] {
            let event = events.iter().find(|event| {
                event.subject == subject
                    && event.reason == reason
                    && matches!(event.change, Some((Change::Resource { account: a, flow: f }, _, _)) if a == account && f == flow)
            }).unwrap();
            let (_, before, after) = event.change.unwrap();
            assert_eq!(before.abs_diff(after), amount);
            assert!((minute..minute + 30).contains(&event.minute));
        }
    }

    #[test]
    fn irrigation_preserves_the_last_drinking_rations_and_skips_rainy_days() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.pump_condition = 20;
        simulation.world.set_balance(Account::Water, 101);
        let report = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(report.water_shortfall, 0);
        assert_eq!(report.water_requested, 113);
        assert_eq!(simulation.world.balance(Account::Water), 0);
        let irrigation = simulation.events()[report.events]
            .iter()
            .find(|event| event.reason == "seedbed_irrigation")
            .unwrap();
        assert!(matches!(irrigation.change, Some((_, 101, 100))));

        let report = simulation.advance_with_weather(Weather::Rain);
        assert_eq!(report.water_shortfall, 0);
        assert!(
            !simulation.events()[report.events]
                .iter()
                .any(|event| event.reason == "seedbed_irrigation")
        );
    }

    #[test]
    fn grid_telemetry_depends_on_served_load_and_source_equipment() {
        let light = GridReport::dispatch(14, 0, 12, 10);
        let heavy = GridReport::dispatch(14, 0, 12, 2);
        assert!(heavy.voltage < light.voltage);
        assert!(heavy.frequency_centihz < light.frequency_centihz);

        let after_shedding = GridReport::dispatch(14, 0, 20, 10);
        assert_eq!(after_shedding.voltage, light.voltage);
        assert_eq!(after_shedding.frequency_centihz, light.frequency_centihz);

        let inverter = GridReport::dispatch(9, 0, 8, 1);
        let mixed = GridReport::dispatch(5, 4, 8, 1);
        assert!(mixed.voltage < inverter.voltage);
        assert!(mixed.frequency_centihz < inverter.frequency_centihz);
    }

    #[test]
    fn water_quality_blends_net_water_and_reflects_rain_and_pump_wear() {
        let mut clear = Simulation::broken_bridge(42);
        clear.advance_with_weather(Weather::Clear);
        assert_eq!(clear.world().water_quality(), 93);

        let mut rain = Simulation::broken_bridge(42);
        rain.world.set_balance(Account::Water, 0);
        let report = rain.advance_with_weather(Weather::Rain);
        assert_eq!(rain.world().water_quality(), 75);
        assert!(
            rain.events()[report.events]
                .iter()
                .any(|event| { event.change == Some((Change::WaterQuality, 95, 75)) })
        );

        let mut worn = Simulation::broken_bridge(42);
        worn.world.set_balance(Account::Water, 0);
        worn.world.pump_condition = 40;
        worn.advance_with_weather(Weather::Rain);
        assert_eq!(worn.world().water_quality(), 65);
    }

    #[test]
    fn low_quality_water_only_makes_residents_who_drink_it_ill() {
        assert_eq!(water_illness_per_mille(60), 0);
        assert!(water_illness_per_mille(40) > water_illness_per_mille(59));
        let seed = (0..1_000)
            .find(|&seed| Random::new(seed).below(1_000) < water_illness_per_mille(59))
            .unwrap();
        for quality in [59, 60] {
            let mut simulation = Simulation::broken_bridge(42);
            simulation.world.pump_condition = 0;
            simulation.world.set_balance(Account::Water, 0);
            simulation.world.mix_treated_water(1, quality);
            simulation.world.set_balance(Account::Water, 1);
            simulation.illness_random = Random::new(seed);
            let report = simulation.advance_with_weather(Weather::Rain);
            assert_eq!(report.water_shortfall, 99);
            let cases: Vec<_> = simulation.events()[report.events]
                .iter()
                .filter(|event| event.reason == "waterborne_illness")
                .collect();
            if quality == 59 {
                assert_eq!(cases.len(), 1);
                assert_eq!(cases[0].subject, Subject::Resident(1));
                assert_eq!(cases[0].change, Some((Change::Health, 100, 85)));
                let ration = &simulation.events()[cases[0].cause.unwrap() - 1];
                assert_eq!(ration.reason, "rations_distributed");
                assert_eq!(report.treated, 1);
            } else {
                assert!(cases.is_empty());
            }
        }
    }

    #[test]
    fn rain_without_fuel_traces_thirst_damage_to_power_denial() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.set_balance(Account::Fuel, 0);
        simulation.world.set_balance(Account::Water, 0);

        let report = simulation.advance_with_weather(Weather::Rain);
        assert_eq!(report.water_shortfall, 100);
        assert_eq!(report.water_requested, 100);
        assert_eq!(
            report.grid,
            GridReport {
                renewable: 5,
                backup: 0,
                requested: 6,
                delivered: 2,
                rated: 22,
                voltage: 232,
                frequency_centihz: 5_020,
            }
        );
        assert_eq!(simulation.world.pump_condition, 70);
        let log = simulation.events();
        let damage = log
            .iter()
            .find(|event| event.reason == "thirst_damage")
            .unwrap();
        let missed = &log[damage.cause.unwrap() - 1];
        let shortfall = &log[missed.cause.unwrap() - 1];
        let denial = &log[shortfall.cause.unwrap() - 1];
        assert_eq!(missed.reason, "water_missed");
        assert_eq!(shortfall.reason, "water_shortfall");
        assert_eq!(denial.reason, "water_power_denied");
        assert_eq!(totals(log, Account::Water).produced, 0);
        assert!(
            simulation.world.residents[..4]
                .iter()
                .all(|r| r.fatigue == 0)
        );
    }

    #[test]
    fn fuel_yard_replenishes_on_clear_days_and_backup_grid_draws_that_stock() {
        let mut simulation = Simulation::broken_bridge(42);
        let full = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(
            totals(&simulation.events()[full.events], Account::Fuel).produced,
            0
        );
        simulation.world.set_balance(Account::Fuel, 0);

        let clear = simulation.advance_with_weather(Weather::Clear);
        let entries = &simulation.events()[clear.events];
        let fuel = entries
            .iter()
            .find(|event| event.reason == "biomass_fuel_produced")
            .unwrap();
        assert_eq!(fuel.subject, Subject::FuelYard);
        assert!((840..870).contains(&fuel.minute));
        assert_eq!(
            totals(entries, Account::Fuel).produced,
            u64::from(FUEL_BATCH)
        );
        assert_eq!(simulation.world.balance(Account::Fuel), FUEL_BATCH);

        let rain = simulation.advance_with_weather(Weather::Rain);
        let entries = &simulation.events()[rain.events];
        assert_eq!(totals(entries, Account::Fuel).produced, 0);
        assert_eq!(
            totals(entries, Account::Fuel).consumed,
            u64::from(FUEL_BATCH)
        );
        assert_eq!(totals(entries, Account::Power).produced, 9);
        assert_eq!(simulation.world.balance(Account::Fuel), 0);
    }

    #[test]
    fn backup_fuel_runs_water_and_repair_on_rainy_day() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.pump_condition = 50;

        let report = simulation.advance_with_weather(Weather::Rain);
        assert_eq!(
            report.grid,
            GridReport {
                renewable: 5,
                backup: 4,
                requested: 8,
                delivered: 8,
                rated: 22,
                voltage: 223,
                frequency_centihz: 4_958,
            }
        );

        let log = simulation.events();
        assert_eq!(totals(log, Account::Fuel).consumed, u64::from(FUEL_BATCH));
        assert_eq!(totals(log, Account::Power).produced, 9);
        assert_eq!(totals(log, Account::Power).consumed, 9);
        assert_eq!(totals(log, Account::Water).produced, 140);
        assert_eq!(totals(log, Account::Wood).consumed, 1);
        assert_eq!(totals(log, Account::Steel).consumed, 1);
        assert_eq!(simulation.world.pump_condition, 64);
    }

    #[test]
    fn repair_restores_production_on_the_next_day() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.pump_condition = 20;

        let stopped = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(
            totals(&simulation.events()[stopped.events], Account::Water).produced,
            0
        );
        assert_eq!(simulation.world.pump_condition, 36);
        let restarted = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(
            totals(&simulation.events()[restarted.events], Account::Water).produced,
            140
        );
    }

    #[test]
    fn exhausted_water_workers_refuse_and_recover() {
        let mut simulation = Simulation::broken_bridge(42);
        for resident in &mut simulation.world.residents {
            if matches!(resident.trade, Trade::Waterworks | Trade::General) {
                resident.fatigue = 70;
            }
        }
        let report = simulation.advance_with_weather(Weather::Clear);
        assert!(report.unfilled_shifts >= 4);
        assert_eq!(totals(simulation.events(), Account::Water).produced, 0);
        assert_eq!(simulation.world.residents[0].fatigue, 50);
        assert!(simulation.events().iter().any(|event| {
            event.subject == Subject::Resident(1) && event.reason == "shift_refused_fatigue"
        }));
    }

    #[test]
    fn injuries_use_shift_and_opening_fatigue() {
        let seed = (0..10_000)
            .find(|&seed| Random::new(seed).below(100) == 7)
            .unwrap();
        let mut simulation = Simulation::broken_bridge(42);
        simulation.random = Random::new(seed);
        simulation.world.residents[0].fatigue = 60;

        simulation.advance_with_weather(Weather::Clear);

        // A roll of 7 is outside the 7% band at fatigue 60. At fatigue 72
        // after the shift it would have counted as a hit.
        assert_eq!(simulation.world.residents[0].health, 100);
        assert_eq!(simulation.world.residents[0].fatigue, 72);
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.residents[0].fatigue = 69;
        let seed = (0..10_000)
            .find(|&seed| Random::new(seed).below(100) == 6)
            .unwrap();
        simulation.random = Random::new(seed);
        let report = simulation.advance_with_weather(Weather::Clear);
        let log = simulation.events();
        let injury = log[report.events]
            .iter()
            .find(|event| event.subject == Subject::Resident(1) && event.reason == "work_injury")
            .unwrap();
        assert_eq!(injury.change, Some((Change::Health, 100, 85)));
        assert_eq!(log[injury.cause.unwrap() - 1].reason, "shift_completed");
    }

    #[test]
    fn kits_require_each_input_and_a_free_staff_slot() {
        let mut simulation = Simulation::broken_bridge(42);
        stop_food_production(&mut simulation);
        simulation.world.set_balance(Account::Medicine, 0);
        simulation.world.set_balance(Account::Cloth, 1);
        let day = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[day.events];
        assert_eq!(day.kits_prepared, 1);
        assert_eq!(totals(events, Account::Medicine).produced, 1);
        for reason in ["kit_cloth_used", "kit_antiseptic_used", "kit_water_used"] {
            let input = events.iter().find(|event| event.reason == reason).unwrap();
            let (_, before, after) = input.change.unwrap();
            assert_eq!(before - after, 1);
        }
        stop_food_production(&mut simulation);
        let next = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(next.kits_prepared, 0);
        assert_eq!(simulation.world.balance(Account::Cloth), 0);

        simulation.world.set_balance(Account::Cloth, 24);
        simulation.world.set_balance(Account::Medicine, 3);
        for resident in &mut simulation.world.residents[..3] {
            resident.health = 50;
        }
        let busy_clinic = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(busy_clinic.treated, 3);
        assert_eq!(busy_clinic.kits_prepared, 0);
        assert_eq!(simulation.world.balance(Account::Cloth), 24);
    }

    #[test]
    fn clinic_respects_staff_and_stock_limits_and_prioritizes_severity() {
        let mut simulation = Simulation::broken_bridge(42);
        for (resident, health) in simulation.world.residents.iter_mut().zip([50, 30, 40, 60]) {
            resident.health = health;
        }
        let first = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(
            (
                first.medical_capacity,
                first.treated,
                first.waiting_for_care
            ),
            (3, 3, 1)
        );
        assert_eq!(simulation.world.balance(Account::Medicine), 21);
        let treated = simulation.events()[first.events]
            .iter()
            .filter(|event| event.reason == "medical_treatment")
            .map(|event| event.subject)
            .collect::<Vec<_>>();
        assert_eq!(
            treated,
            [
                Subject::Resident(2),
                Subject::Resident(3),
                Subject::Resident(1)
            ]
        );
        simulation.world.set_balance(Account::Medicine, 1);
        simulation.world.set_balance(Account::Cloth, 0);
        stop_food_production(&mut simulation);
        let next = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(next.treated, 1);
        assert!(next.waiting_for_care > 0);
        assert_eq!(simulation.world.balance(Account::Medicine), 0);
    }

    #[test]
    fn ration_priority_rotates_and_dead_residents_do_not_consume() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.set_balance(Account::Medicine, 0);
        for resident in &mut simulation.world.residents {
            resident.health = 40;
        }
        simulation.world.residents[RESIDENT_COUNT - 1].health = 0;
        simulation.world.set_balance(Account::Food, 1);
        simulation.world.set_balance(Account::Water, 1);
        let first = simulation.advance_with_weather(Weather::Clear);
        assert_eq!((first.food_shortfall, first.water_shortfall), (98, 98));
        assert_eq!(simulation.world.residents[0].thirst, 0);

        simulation.world.set_balance(Account::Water, 1);
        simulation.advance_with_weather(Weather::Clear);
        assert_eq!(simulation.world.residents[0].thirst, 35);
        assert_eq!(simulation.world.residents[1].thirst, 15);
        assert_eq!(simulation.world.residents[RESIDENT_COUNT - 1].thirst, 0);
    }

    #[test]
    fn substitute_repair_keeps_the_pump_running_without_steel() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.pump_condition = 50;
        for (index, plot) in simulation.world().plots().to_vec().iter().enumerate() {
            if plot.accessible {
                simulation
                    .world
                    .change_plot(index, Account::FieldSteel, -i64::from(plot.steel));
            }
        }
        simulation.world.set_balance(Account::Steel, 0);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events.clone()];
        assert_eq!(simulation.world.pump_condition, 58);
        assert_eq!(totals(events, Account::Wood).consumed, 1);
        assert_eq!(totals(events, Account::Steel).consumed, 0);
        assert!(
            events
                .iter()
                .any(|event| event.reason == "repair_substitute")
        );
        let restarted = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(
            totals(&simulation.events()[restarted.events], Account::Water).produced,
            140
        );
    }

    #[test]
    fn fishing_vote_reallocates_short_crews_and_preserves_full_crews() {
        let first_winter = (1..=365)
            .find(|&day| season_of(day) == Season::Winter)
            .unwrap();
        let run = |approve, available| {
            let mut simulation = Simulation::broken_bridge(42);
            simulation.world.day = first_winter - 1;
            if approve {
                // Three supporters and one opponent attend this tied-yield vote.
                for id in [1, 2, 17] {
                    simulation.world.residents[id - 1].health = 25;
                }
            }
            let first = simulation.advance_with_weather(Weather::Rain);
            let expected = if approve {
                "food_vote_passed"
            } else {
                "food_vote_rejected"
            };
            assert!(
                simulation.events()[first.events]
                    .iter()
                    .any(|event| event.reason == expected)
            );
            simulation.world.set_balance(Account::Food, 400);
            let mut food_workers = 0;
            for resident in &mut simulation.world.residents {
                resident.health = 100;
                resident.fatigue = match resident.trade {
                    Trade::Food if food_workers < available => {
                        food_workers += 1;
                        0
                    }
                    Trade::Food | Trade::General => 70,
                    _ => 0,
                };
            }
            let second = simulation.advance_with_weather(Weather::Clear);
            let events = &simulation.events()[second.events];
            let allocation = events
                .iter()
                .find(|event| event.reason == "food_crew_allocated")
                .unwrap();
            let Some((Change::FoodCrew, farm, fish)) = allocation.change else {
                panic!("missing crew counts")
            };
            let output = |reason| {
                events
                    .iter()
                    .find(|event| event.reason == reason)
                    .map_or(0, |event| {
                        let (_, before, after) = event.change.unwrap();
                        after - before
                    })
            };
            if approve && available < FIELD_SHIFTS + FISHING_SHIFTS {
                let catch = events
                    .iter()
                    .find(|event| event.reason == "fish_landed")
                    .unwrap();
                assert_eq!(catch.cause, Some(allocation.id));
                let mut trace = Some(catch.id);
                while let Some(id) = trace {
                    let event = &simulation.events()[id - 1];
                    if event.reason == "food_vote_passed" {
                        break;
                    }
                    trace = event.cause;
                }
                assert!(trace.is_some());
            }
            (farm, fish, output("food_production"), output("fish_landed"))
        };
        assert_eq!(run(false, 10), (10, 0, 50, 0));
        assert_eq!(run(true, 10), (3, 7, 15, 42));
        assert_eq!(run(true, 5), (0, 5, 0, 30));
        assert_eq!(run(false, 20), (13, 7, 65, 42));
        assert_eq!(run(true, 20), (13, 7, 65, 42));
    }

    #[test]
    fn council_vote_controls_next_day_water_and_fuel_with_emergency_override() {
        let run = |absentees: &[usize], water| {
            let mut simulation = Simulation::broken_bridge(42);
            simulation.world.set_balance(Account::Fuel, 80);
            simulation.world.pump_condition = 60;
            for &id in absentees {
                simulation.world.residents[id - 1].health = 25;
            }
            let first = simulation.advance_with_weather(Weather::Clear);
            assert_eq!(first.backup_quota, BACKUP_BATCH_LIMIT);
            for &id in absentees {
                simulation.world.residents[id - 1].health = 100;
            }
            simulation.world.set_balance(Account::Water, water);
            let second = simulation.advance_with_weather(Weather::Rain);
            (simulation, second)
        };

        let (approved, approved_day) = run(&[1, 2, 17], 400);
        let (rejected, rejected_day) = run(&[], 400);
        let (emergency, emergency_day) = run(&[1, 2, 17], WATER_RESERVE);
        let (no_quorum, _) = run(&[1, 2, 17, 18], 400);
        let has = |simulation: &Simulation, reason| {
            simulation
                .events()
                .iter()
                .any(|event| event.reason == reason)
        };
        assert!(has(&approved, "backup_vote_passed"));
        assert!(has(&rejected, "backup_vote_rejected"));
        assert!(has(&no_quorum, "backup_vote_no_quorum"));
        assert!(has(&emergency, "backup_reserve_override"));
        assert_eq!(approved_day.backup_quota, 0);
        assert_eq!(rejected_day.backup_quota, BACKUP_BATCH_LIMIT);
        assert_eq!(emergency_day.backup_quota, BACKUP_BATCH_LIMIT);
        let events = approved.events();
        let denied = events[approved_day.events.clone()]
            .iter()
            .find(|event| event.reason == "water_power_denied")
            .unwrap();
        let blocked = &events[denied.cause.unwrap() - 1];
        let effective = &events[blocked.cause.unwrap() - 1];
        let vote = &events[effective.cause.unwrap() - 1];
        assert_eq!(blocked.reason, "backup_blocked_by_vote");
        assert_eq!(effective.reason, "backup_limit_effective");
        assert_eq!(vote.reason, "backup_vote_passed");
        assert_eq!(vote.day + 1, denied.day);
        for (account, flow) in [
            (Account::Fuel, Flow::Consumed),
            (Account::Water, Flow::Produced),
        ] {
            let measurement = |events: &[Event]| {
                let total = totals(events, account);
                if flow == Flow::Consumed {
                    total.consumed
                } else {
                    total.produced
                }
            };
            assert_eq!(measurement(&events[approved_day.events.clone()]), 0);
            assert!(measurement(&rejected.events()[rejected_day.events.clone()]) > 0);
        }
    }

    #[test]
    fn eligible_pump_crew_can_refuse_a_risky_shift() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.set_balance(Account::Fuel, 80);
        simulation.advance_with_weather(Weather::Clear);
        let relationships: Vec<_> = simulation
            .relationships()
            .filter(|(a, b, _)| (*a, *b) == (1, 2))
            .collect();
        simulation.world.pump_condition = 25;
        for resident in &mut simulation.world.residents {
            resident.health = 65;
        }
        let report = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(report.backup_quota, 0);
        let events = &simulation.events()[report.events];
        assert!(
            events
                .iter()
                .any(|event| event.reason == "shift_refused_risk")
        );
        assert!(events.iter().any(|event| event.reason == "no_water_crew"));
        assert_eq!(totals(events, Account::Water).produced, 0);
        assert_eq!(
            simulation
                .relationships()
                .filter(|(a, b, _)| (*a, *b) == (1, 2))
                .collect::<Vec<_>>(),
            relationships
        );
    }

    #[test]
    fn familiar_coworker_is_offered_a_shift_before_unfamiliar_peers() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.social.work_together(0, 43, 0);
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events];
        let priority = events
            .iter()
            .find(|event| event.reason == "crew_familiarity_priority")
            .unwrap();
        assert_eq!(priority.subject, Subject::Resident(44));
        assert_eq!(priority.change, Some((Change::CrewPriority, 0, 1)));
        assert_ne!(priority.related_resident_id, Some(44));
        let accepted = events
            .iter()
            .find(|event| event.cause == Some(priority.id))
            .unwrap();
        assert_eq!(accepted.reason, "shift_accepted");
        assert_eq!(accepted.subject, Subject::Resident(44));
        assert!(
            simulation
                .relationships()
                .any(|(a, b, pair)| (a, b, pair.completed_together) == (1, 44, 2))
        );
    }

    #[test]
    fn recent_cooperation_changes_natural_crews_under_the_same_weather() {
        for seed in [0, 7, 42, 123] {
            let mut enabled = Simulation::broken_bridge(seed);
            let mut disabled = Simulation::broken_bridge(seed);
            disabled.set_familiarity_enabled(false);
            let mut changed_days = 0;
            for _ in 0..500 {
                let left = enabled.advance();
                let right = disabled.advance();
                assert_eq!(left.weather, right.weather);
                let normalize = |mut crews: Vec<Vec<usize>>| {
                    for crew in &mut crews {
                        crew.sort_unstable();
                    }
                    crews.sort_unstable();
                    crews
                };
                changed_days += usize::from(
                    normalize(left.completed_crews) != normalize(right.completed_crews),
                );
            }
            assert!(changed_days > 0, "seed {seed}");
            assert!(
                enabled
                    .events()
                    .iter()
                    .any(|event| event.reason == "crew_familiarity_priority")
            );
            assert!(
                !disabled
                    .events()
                    .iter()
                    .any(|event| event.reason == "crew_familiarity_priority")
            );
        }
    }

    #[test]
    fn social_state_and_events_reproduce_over_five_hundred_days() {
        let mut left = Simulation::broken_bridge(42);
        let mut right = Simulation::broken_bridge(42);
        for _ in 0..500 {
            assert_eq!(left.advance(), right.advance());
        }
        assert_eq!(left.world(), right.world());
        assert_eq!(left.social, right.social);
        assert_eq!(left.events(), right.events());
        // The enlarged salvage reserve keeps the fuel yard near its target, so
        // the council never has to ration backup power. Food votes still
        // exercise the social layer end to end.
        assert!(
            left.events()
                .iter()
                .any(|event| event.reason == "food_vote_passed")
        );
        assert!(left.relationships().next().is_some());
    }

    #[test]
    fn five_hundred_days_sustain_the_settlement() {
        for seed in [0, 7, 42, 123, u64::MAX] {
            let mut simulation = Simulation::broken_bridge(seed);
            for date in 1..=500 {
                let report = simulation.advance();
                assert_eq!(report.day, date);
                assert_eq!(
                    report.population, RESIDENT_COUNT,
                    "seed {seed} lost residents by day {date}"
                );
                assert_eq!(report.food_shortfall, 0, "seed {seed} day {date} food");
                assert_eq!(report.water_shortfall, 0, "seed {seed} day {date} water");
            }
            let world = simulation.world();
            assert!(world.pump_condition > 20, "seed {seed} pump stalled");
            assert!(world.granary_condition > 0, "seed {seed} granary collapsed");
            assert!(
                world.balance(Account::Medicine) > 0,
                "seed {seed} ran out of kits"
            );
            assert!(
                world.balance(Account::Cloth) > 0,
                "seed {seed} ran out of cloth"
            );
            assert!(
                world.balance(Account::Antiseptic) > 0,
                "seed {seed} ran out of antiseptic"
            );
        }
    }

    #[test]
    fn dehydration_kills_residents_when_the_pump_cannot_run() {
        let mut simulation = Simulation::broken_bridge(42);
        simulation.world.pump_condition = 20;
        simulation.world.set_balance(Account::Wood, 0);
        for (index, plot) in simulation.world().plots().to_vec().iter().enumerate() {
            if plot.accessible {
                simulation
                    .world
                    .change_plot(index, Account::FieldWood, -i64::from(plot.wood));
            }
        }
        let mut deaths = 0;
        for _ in 0..40 {
            let report = simulation.advance_with_weather(Weather::Clear);
            deaths += simulation.events()[report.events]
                .iter()
                .filter(|event| event.reason == "resident_died")
                .count();
        }
        assert!(deaths > 0, "a settlement without water must bury its dead");
        assert!(
            simulation
                .world()
                .residents
                .iter()
                .all(|resident| !resident.is_alive())
        );
    }

    #[test]
    fn season_boundaries_follow_the_calendar() {
        assert_eq!(season_of(1), Season::Spring);
        assert_eq!(season_of(61), Season::Spring);
        assert_eq!(season_of(62), Season::Summer);
        assert_eq!(season_of(153), Season::Summer);
        assert_eq!(season_of(154), Season::Autumn);
        assert_eq!(season_of(244), Season::Autumn);
        assert_eq!(season_of(245), Season::Winter);
        assert_eq!(season_of(335), Season::Winter);
        assert_eq!(season_of(336), Season::Spring);
        assert!(
            Season::Autumn.farm_yield(Weather::Clear) > Season::Spring.farm_yield(Weather::Clear)
        );
        assert!(
            Season::Spring.farm_yield(Weather::Clear) > Season::Winter.farm_yield(Weather::Clear)
        );
        assert!(
            Season::Autumn.farm_yield(Weather::Rain) > Season::Spring.farm_yield(Weather::Rain)
        );
    }

    fn harvest_with_full_crews(simulation: &mut Simulation) -> (u32, u32) {
        for resident in &mut simulation.world.residents {
            resident.health = 100;
            resident.fatigue = 0;
            resident.hunger = 0;
            resident.thirst = 0;
        }
        let report = simulation.advance_with_weather(Weather::Clear);
        let events = &simulation.events()[report.events.clone()];
        let farm = events
            .iter()
            .find(|event| event.reason == "food_production")
            .unwrap();
        let catch = events
            .iter()
            .find(|event| event.reason == "fish_landed")
            .unwrap();
        let (_, farm_before, farm_after) = farm.change.unwrap();
        let (_, catch_before, catch_after) = catch.change.unwrap();
        (farm_after - farm_before, catch_after - catch_before)
    }

    #[test]
    fn farm_output_follows_the_season_while_fishing_stays_steady() {
        let mut simulation = Simulation::broken_bridge(42);
        assert_eq!(harvest_with_full_crews(&mut simulation), (13 * 6, 7 * 6));
        for _ in 0..60 {
            simulation.advance_with_weather(Weather::Clear);
        }
        assert_eq!(harvest_with_full_crews(&mut simulation), (13 * 7, 7 * 6));
        for _ in 0..91 {
            simulation.advance_with_weather(Weather::Clear);
        }
        assert_eq!(harvest_with_full_crews(&mut simulation), (13 * 8, 7 * 6));
        for _ in 0..90 {
            simulation.advance_with_weather(Weather::Clear);
        }
        assert_eq!(harvest_with_full_crews(&mut simulation), (13 * 5, 7 * 6));
    }

    #[test]
    fn summer_algae_lowers_treated_water_quality_and_adds_a_water_load() {
        let mut simulation = Simulation::broken_bridge(42);
        for _ in 0..61 {
            simulation.advance_with_weather(Weather::Clear);
        }
        simulation.world.set_balance(Account::Water, 0);
        simulation.world.set_balance(Account::Cloth, 24);
        simulation.world.set_balance(Account::Antiseptic, 24);
        simulation.world.pump_condition = 70;
        let report = simulation.advance_with_weather(Weather::Clear);
        assert_eq!(report.day, 62);
        assert_eq!(report.season, Season::Summer);
        // 97 - (100 - 70) / 3 - 32 = 55
        assert_eq!(simulation.world().water_quality(), 55);
        assert_eq!(report.grid.requested, 7);
        assert!(
            simulation.events()[report.events]
                .iter()
                .any(|event| event.reason == "algae_bloom")
        );

        let spring = {
            let mut simulation = Simulation::broken_bridge(42);
            simulation.world.set_balance(Account::Water, 0);
            simulation.world.pump_condition = 70;
            let report = simulation.advance_with_weather(Weather::Clear);
            assert_eq!(report.season, Season::Spring);
            simulation.world().water_quality()
        };
        assert_eq!(spring, 87);
        assert!(spring > 55);
    }
}
