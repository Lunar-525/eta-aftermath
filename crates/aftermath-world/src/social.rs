//! Broken Bridge's membership rolls and relationships.

use crate::world::{RESIDENT_COUNT, Resident, Trade};

pub const COUNCIL: [usize; 7] = [1, 2, 7, 17, 18, 21, 22];
pub const COOPERATIVE: [usize; 5] = [1, 3, 7, 17, 18];
pub const EXCHANGE: [usize; 4] = [21, 22, 23, 24];

pub(crate) const WATER_RESERVE: u32 = 200;
pub(crate) const FUEL_REVIEW: u32 = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Social {
    relationships: Vec<Relationship>,
    recent_work: [u16; RESIDENT_COUNT],
    pub(crate) use_familiarity: bool,
    pub(crate) approved_limit: Option<usize>,
    pub(crate) approved_fishing_priority: Option<usize>,
}

impl Default for Social {
    fn default() -> Self {
        Self {
            relationships: vec![Relationship::default(); RESIDENT_COUNT * RESIDENT_COUNT],
            recent_work: [0; RESIDENT_COUNT],
            use_familiarity: true,
            approved_limit: None,
            approved_fishing_priority: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Relationship {
    pub completed_together: u32,
    pub last_day: u32,
}

pub const COOPERATION_WINDOW: u32 = 14;

impl Social {
    fn pair_index(left: usize, right: usize) -> usize {
        left.min(right) * RESIDENT_COUNT + left.max(right)
    }

    pub(crate) fn relationships(&self) -> impl Iterator<Item = (usize, usize, Relationship)> + '_ {
        self.relationships
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, pair)| {
                (pair.completed_together > 0).then_some((
                    index / RESIDENT_COUNT + 1,
                    index % RESIDENT_COUNT + 1,
                    pair,
                ))
            })
    }

    pub(crate) fn affinity(&self, left: usize, right: usize, day: u32) -> u32 {
        let pair = self.relationships[Self::pair_index(left, right)];
        if pair.completed_together == 0 || !self.use_familiarity {
            return 0;
        }
        (COOPERATION_WINDOW + 1).saturating_sub(day - pair.last_day)
    }

    pub(crate) fn work_together(&mut self, left: usize, right: usize, day: u32) -> u32 {
        let pair = &mut self.relationships[Self::pair_index(left, right)];
        pair.completed_together += 1;
        pair.last_day = day;
        pair.completed_together
    }

    pub(crate) fn workload(&self, index: usize) -> u32 {
        self.recent_work[index].count_ones()
    }

    pub(crate) fn close_workday(&mut self, worked: &[Option<usize>; RESIDENT_COUNT]) {
        for (history, shift) in self.recent_work.iter_mut().zip(worked) {
            const WINDOW_MASK: u16 = (1 << COOPERATION_WINDOW) - 1;
            *history = ((*history << 1) | u16::from(shift.is_some())) & WINDOW_MASK;
        }
    }
}

pub(crate) fn can_attend(resident: &Resident) -> bool {
    resident.health > 40 && resident.fatigue < 70
}

pub(crate) fn supports_limit(resident: &Resident, pump: u32, quality: u32) -> bool {
    match resident.trade {
        Trade::Waterworks => pump >= 60 && quality >= 80,
        Trade::Mechanic => pump >= 60,
        Trade::Food | Trade::General => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_cooperation_expires_but_the_lifetime_count_remains() {
        let mut social = Social::default();
        for date in 1..=8 {
            social.work_together(0, 1, date);
        }
        assert_eq!(social.affinity(0, 1, 9), 14);
        assert_eq!(social.affinity(1, 0, 22), 1);
        assert_eq!(social.affinity(1, 0, 23), 0);
        assert_eq!(
            social.relationships().next().unwrap().2.completed_together,
            8
        );
    }
}
