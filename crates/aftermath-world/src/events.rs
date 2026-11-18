//! Event records and accounting. Resource changes are applied through the ledger.

use std::io::{self, Write};

use crate::world::{Account, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Grid,
    Council,
    Cooperative,
    Exchange,
    FuelYard,
    Pump,
    Farm,
    Granary,
    Fishery,
    Archive,
    Clinic,
    Salvage,
    Plot(usize),
    Settlement,
    Resident(usize),
}

impl std::fmt::Display for Subject {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Grid => output.write_str("grid"),
            Self::Council => output.write_str("council"),
            Self::Cooperative => output.write_str("cooperative"),
            Self::Exchange => output.write_str("exchange"),
            Self::FuelYard => output.write_str("fuel-yard"),
            Self::Pump => output.write_str("pump"),
            Self::Farm => output.write_str("farm"),
            Self::Granary => output.write_str("granary"),
            Self::Fishery => output.write_str("fishery"),
            Self::Archive => output.write_str("archive"),
            Self::Clinic => output.write_str("clinic"),
            Self::Salvage => output.write_str("salvage"),
            Self::Plot(id) => write!(output, "plot-{id}"),
            Self::Settlement => output.write_str("settlement"),
            Self::Resident(id) => write!(output, "resident-{id}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Produced,
    Consumed,
    TransferIn,
    TransferOut,
}

impl Flow {
    fn name(self) -> &'static str {
        match self {
            Self::Produced => "produced",
            Self::Consumed => "consumed",
            Self::TransferIn => "transfer_in",
            Self::TransferOut => "transfer_out",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Resource { account: Account, flow: Flow },
    Condition,
    Hunger,
    Thirst,
    Fatigue,
    Health,
    FoodQuantity,
    WaterQuality,
    Unfilled,
    BackupFuel,
    ProposalStock,
    ProposalCondition,
    VoteTally,
    BackupQuota,
    Familiarity,
    CrewPriority,
    FoodCrew,
    HarvestRates,
}

impl Change {
    fn name(self) -> &'static str {
        match self {
            Self::Resource { flow, .. } => flow.name(),
            Self::Condition => "condition",
            Self::Hunger => "hunger",
            Self::Thirst => "thirst",
            Self::Fatigue => "fatigue",
            Self::Health => "health",
            Self::FoodQuantity => "food_quantity",
            Self::WaterQuality => "water_quality",
            Self::Unfilled => "unfilled",
            Self::BackupFuel => "backup_fuel",
            Self::ProposalStock => "proposal_stock",
            Self::ProposalCondition => "proposal_condition",
            Self::VoteTally => "vote_tally",
            Self::BackupQuota => "backup_quota",
            Self::Familiarity => "familiarity",
            Self::CrewPriority => "crew_priority",
            Self::FoodCrew => "food_crew",
            Self::HarvestRates => "harvest_rates",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub id: usize,
    pub day: u32,
    pub minute: u16,
    pub cause: Option<usize>,
    pub subject: Subject,
    pub resident_id: Option<usize>,
    pub related_resident_id: Option<usize>,
    pub reason: &'static str,
    pub change: Option<(Change, u32, u32)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub produced: u64,
    pub consumed: u64,
    pub received: u64,
    pub sent: u64,
}

#[derive(Default)]
pub(crate) struct Journal {
    events: Vec<Event>,
    minute: u16,
}

impl Journal {
    pub(crate) fn events(&self) -> &[Event] {
        &self.events
    }

    pub(crate) fn at(&mut self, hour: u16, minute: u16) {
        debug_assert!(hour < 24 && minute < 60);
        self.minute = hour * 60 + minute;
    }

    pub(crate) fn record(
        &mut self,
        day: u32,
        subject: Subject,
        reason: &'static str,
        cause: Option<usize>,
        change: Option<(Change, u32, u32)>,
    ) -> usize {
        let id = self.events.len() + 1;
        debug_assert!(cause.is_none_or(|previous| previous > 0 && previous < id));
        debug_assert!(
            self.events
                .last()
                .is_none_or(|previous| { (previous.day, previous.minute) <= (day, self.minute) })
        );
        self.events.push(Event {
            id,
            day,
            minute: self.minute,
            subject,
            resident_id: match subject {
                Subject::Resident(id) => Some(id),
                _ => None,
            },
            related_resident_id: None,
            reason,
            cause,
            change,
        });
        id
    }

    pub(crate) fn record_pair(
        &mut self,
        day: u32,
        (resident, related): (usize, usize),
        reason: &'static str,
        cause: Option<usize>,
        change: (Change, u32, u32),
    ) -> usize {
        let id = self.record(
            day,
            Subject::Resident(resident),
            reason,
            cause,
            Some(change),
        );
        self.events[id - 1].related_resident_id = Some(related);
        id
    }

    pub(crate) fn post(
        &mut self,
        world: &mut World,
        subject: Subject,
        reason: &'static str,
        cause: Option<usize>,
        entry: (Account, Flow, u32),
    ) -> usize {
        let (account, flow, amount) = entry;
        let before = world.balance(account);
        let after = match flow {
            Flow::Produced | Flow::TransferIn => before + amount,
            Flow::Consumed | Flow::TransferOut => before - amount,
        };
        if matches!(account, Account::FieldWood | Account::FieldSteel) {
            let Subject::Plot(plot) = subject else {
                unreachable!("field inventory must be assigned to a map plot");
            };
            world.change_plot(plot, account, i64::from(after) - i64::from(before));
        } else {
            world.set_balance(account, after);
        }
        self.record(
            world.day,
            subject,
            reason,
            cause,
            Some((Change::Resource { account, flow }, before, after)),
        )
    }
}

pub fn totals(events: &[Event], account: Account) -> Totals {
    let mut totals = Totals::default();
    for event in events {
        let Some((
            Change::Resource {
                account: posted,
                flow,
            },
            before,
            after,
        )) = event.change
        else {
            continue;
        };
        if posted == account {
            let amount = u64::from(after.abs_diff(before));
            match flow {
                Flow::Produced => totals.produced += amount,
                Flow::Consumed => totals.consumed += amount,
                Flow::TransferIn => totals.received += amount,
                Flow::TransferOut => totals.sent += amount,
            }
        }
    }
    totals
}

/// Writes the complete event sequence as CSV. Identifiers and reasons are fixed tokens.
pub fn write_csv(mut output: impl Write, events: &[Event]) -> io::Result<()> {
    writeln!(
        output,
        "id,day,cause,subject,reason,change,account,before,after,time,resident_id,related_resident_id"
    )?;
    for event in events {
        write!(output, "{},{},", event.id, event.day)?;
        if let Some(cause) = event.cause {
            write!(output, "{cause}")?;
        }
        write!(output, ",{},{},", event.subject, event.reason)?;
        if let Some((change, before, after)) = event.change {
            let account = match change {
                Change::Resource { account, .. } => account.name(),
                _ => "",
            };
            write!(output, "{},{account},{before},{after},", change.name())?;
        } else {
            write!(output, ",,,,")?;
        }
        write!(output, "{:02}:{:02},", event.minute / 60, event.minute % 60)?;
        if let Some(id) = event.resident_id {
            write!(output, "{id}")?;
        }
        write!(output, ",")?;
        if let Some(id) = event.related_resident_id {
            write!(output, "{id}")?;
        }
        writeln!(output)?;
    }
    Ok(())
}
