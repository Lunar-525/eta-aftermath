//! Authoritative world state and the opening Broken Bridge scenario.

pub const RESIDENT_COUNT: usize = 100;
pub(crate) const FUEL_BATCH: u32 = 10;
pub(crate) const FUEL_YARD_TARGET: u32 = 24 * FUEL_BATCH;
const GRANARY_RATED_CAPACITY: u32 = 800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Account {
    Food,
    Water,
    Fuel,
    Wood,
    Steel,
    FieldWood,
    FieldSteel,
    Power,
    Medicine,
    Cloth,
    Antiseptic,
}

impl Account {
    pub const ALL: [Self; 11] = [
        Self::Food,
        Self::Water,
        Self::Fuel,
        Self::Wood,
        Self::Steel,
        Self::FieldWood,
        Self::FieldSteel,
        Self::Power,
        Self::Medicine,
        Self::Cloth,
        Self::Antiseptic,
    ];

    /// Physical display scale in tenths of the named unit.
    pub fn measurement(self) -> (u32, &'static str) {
        match self {
            Self::Food => (6, "kg"),
            Self::Water => (30, "L"),
            Self::Fuel => (2, "kg"),
            Self::Wood | Self::Steel | Self::FieldWood | Self::FieldSteel => (50, "kg"),
            Self::Power => (5, "kWh"),
            Self::Medicine => (10, "份"),
            Self::Cloth => (1, "kg"),
            Self::Antiseptic => (1, "L"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Food => "food",
            Self::Water => "water",
            Self::Fuel => "fuel",
            Self::Wood => "wood",
            Self::Steel => "steel",
            Self::FieldWood => "field_wood",
            Self::FieldSteel => "field_steel",
            Self::Power => "power",
            Self::Medicine => "medicine",
            Self::Cloth => "cloth",
            Self::Antiseptic => "antiseptic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trade {
    Waterworks,
    Food,
    Mechanic,
    General,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resident {
    pub id: usize,
    pub name: &'static str,
    pub age: u32,
    pub trade: Trade,
    pub hunger: u32,
    pub thirst: u32,
    pub fatigue: u32,
    pub health: u32,
}

impl Resident {
    pub fn is_alive(&self) -> bool {
        self.health > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldPlot {
    pub col: u8,
    pub row: u8,
    pub wood: u32,
    pub steel: u32,
    pub wood_capacity: u32,
    pub accessible: bool,
}

impl FieldPlot {
    const fn new(
        col: u8,
        row: u8,
        wood: u32,
        steel: u32,
        wood_capacity: u32,
        accessible: bool,
    ) -> Self {
        Self {
            col,
            row,
            wood,
            steel,
            wood_capacity,
            accessible,
        }
    }
}

const FIELD_PLOTS: [FieldPlot; 12] = [
    FieldPlot::new(10, 5, 20, 60, 0, true),
    FieldPlot::new(9, 5, 0, 4, 0, true),
    FieldPlot::new(11, 5, 1, 3, 0, true),
    FieldPlot::new(12, 4, 1, 3, 0, true),
    FieldPlot::new(4, 3, 2, 1, 0, true),
    FieldPlot::new(7, 6, 2, 2, 0, true),
    FieldPlot::new(4, 5, 2, 0, 0, true),
    FieldPlot::new(4, 8, 2, 0, 8, true),
    FieldPlot::new(5, 2, 3, 0, 8, true),
    FieldPlot::new(6, 2, 2, 0, 8, true),
    FieldPlot::new(2, 1, 1, 0, 8, true),
    FieldPlot::new(1, 3, 2, 3, 0, false),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    pub day: u32,
    pub residents: [Resident; RESIDENT_COUNT],
    pub pump_condition: u32,
    pub granary_condition: u32,
    water_quality_hundredths: u32,
    balances: [u32; Account::ALL.len()],
    // Most deteriorated stock is first. New harvest is appended at zero.
    food_decay: Vec<u8>,
    plots: [FieldPlot; FIELD_PLOTS.len()],
}

impl World {
    pub(crate) fn broken_bridge(random: &mut Random) -> Self {
        use Trade::{Food, General, Mechanic, Waterworks};

        const NAMES: [&str; RESIDENT_COUNT] = [
            "周静", "沈立", "顾远", "陈月", "蒋诚", "陆雯", "许志", "朱梅", "徐宁", "孙青", "钱平",
            "吴湘", "杨舟", "王岚", "赵禾", "唐澜", "胡铭", "林秋", "高然", "谢勤", "苏瑾", "邵阳",
            "何霁", "严华", "冯霖", "叶石", "曹晓", "金瑜", "姚南", "丁辰", "汪凝", "程珂", "薛松",
            "袁芳", "盛唐", "邹棠", "潘柔", "范宇", "韩晨", "魏穗", "段岩", "罗清", "柳乔", "蒋珺",
            "梁满", "吴辰", "冯竹", "韩晖", "戚安", "孟雪", "戴山", "白云", "卫川", "阮宁", "姜茹",
            "石霖", "方惠", "夏萍", "葛晴", "殷桐", "俞晗", "章奇", "邢野", "马淮", "宋愿", "沈芊",
            "常新", "徐衡", "钱粟", "陆芸", "贺朗", "田野", "裴钰", "谢轻", "蔡萱", "朱熙", "洛华",
            "俞帆", "卢望", "项林", "陶乐", "顾春", "王增", "丁美", "姚森", "于琼", "郑起", "陈涛",
            "宋桦", "武江", "缪笛", "管岳", "纪荫", "董舒", "尹红", "季芳", "伍雄", "洪燕", "叶蕙",
            "褚霞",
        ];

        Self {
            day: 0,
            residents: std::array::from_fn(|index| Resident {
                id: index + 1,
                name: NAMES[index],
                age: 18 + random.below(51),
                trade: match index {
                    0..6 | 30..44 => Waterworks,
                    6..16 | 44..67 => Food,
                    16..20 | 67..76 => Mechanic,
                    _ => General,
                },
                hunger: 0,
                thirst: 0,
                fatigue: 0,
                health: 100,
            }),
            pump_condition: 70,
            granary_condition: 80,
            water_quality_hundredths: 9_500,
            balances: [0, 400, FUEL_YARD_TARGET, 8, 8, 0, 0, 0, 24, 24, 24],
            food_decay: vec![0; 400],
            plots: FIELD_PLOTS,
        }
    }

    pub fn balance(&self, account: Account) -> u32 {
        match account {
            Account::Food => self.food_decay.len() as u32,
            Account::FieldWood => self.plots.iter().map(|plot| plot.wood).sum(),
            Account::FieldSteel => self.plots.iter().map(|plot| plot.steel).sum(),
            _ => self.balances[account as usize],
        }
    }

    pub fn granary_capacity(&self) -> u32 {
        GRANARY_RATED_CAPACITY * (100 + self.granary_condition) / 200
    }

    pub fn water_quality(&self) -> u32 {
        (self.water_quality_hundredths + 50) / 100
    }

    pub(crate) fn mix_treated_water(&mut self, amount: u32, quality: u32) {
        let stored = u64::from(self.balance(Account::Water));
        let new = u64::from(amount);
        let mass =
            u64::from(self.water_quality_hundredths) * stored + u64::from(quality) * 100 * new;
        self.water_quality_hundredths = (mass / (stored + new)) as u32;
    }

    /// Each row contains the counts in successive 20-point deterioration bands.
    /// The warehouse row comes first, followed by food kept outside.
    pub fn food_quality(&self) -> [[u32; 5]; 2] {
        let overflow = self
            .balance(Account::Food)
            .saturating_sub(self.granary_capacity()) as usize;
        let mut profile = [[0; 5]; 2];
        for (index, &decay) in self.food_decay.iter().enumerate() {
            profile[usize::from(index < overflow)][usize::from(decay.min(99) / 20)] += 1;
        }
        profile
    }

    pub(crate) fn age_food(&mut self, stored_decay: u8, exposed_decay: u8) -> (u32, u32) {
        debug_assert!(exposed_decay >= stored_decay);
        let overflow = self
            .balance(Account::Food)
            .saturating_sub(self.granary_capacity());
        let (outside, inside) = self.food_decay.split_at_mut(overflow as usize);
        for (portions, decay) in [(outside, exposed_decay), (inside, stored_decay)] {
            for quality in portions {
                *quality = quality.saturating_add(decay).min(100);
            }
        }
        let spoiled = self
            .food_decay
            .iter()
            .take_while(|&&decay| decay >= 100)
            .count() as u32;
        (overflow, spoiled)
    }

    pub(crate) fn ration_decay(&self, offset: usize) -> u8 {
        self.food_decay[offset]
    }

    pub fn plots(&self) -> &[FieldPlot] {
        &self.plots
    }

    pub(crate) fn change_plot(&mut self, plot: usize, account: Account, delta: i64) {
        let plot = &mut self.plots[plot];
        let amount = match account {
            Account::FieldWood => &mut plot.wood,
            Account::FieldSteel => &mut plot.steel,
            _ => unreachable!(),
        };
        *amount =
            u32::try_from(i64::from(*amount) + delta).expect("plot balance fits a positive u32");
    }

    pub(crate) fn set_balance(&mut self, account: Account, value: u32) {
        debug_assert!(!matches!(account, Account::FieldWood | Account::FieldSteel));
        if account == Account::Food {
            let next = value as usize;
            if next < self.food_decay.len() {
                self.food_decay.drain(..self.food_decay.len() - next);
            } else {
                self.food_decay.resize(next, 0);
            }
            return;
        }
        self.balances[account as usize] = value;
    }
}

/// SplitMix64 with explicit wrapping arithmetic and rejection sampling.
pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn below(&mut self, upper: u32) -> u32 {
        let upper = u64::from(upper);
        let threshold = upper.wrapping_neg() % upper;
        loop {
            self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = self.0;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^= value >> 31;
            if value >= threshold {
                return (value % upper) as u32;
            }
        }
    }
}
