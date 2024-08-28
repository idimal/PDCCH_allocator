#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum PdcchAggregation {
    #[default]
    L1 = 0,
    L2,
    L4,
    L8,
}

impl PdcchAggregation {
    pub const fn size(&self) -> usize {
        match self {
            PdcchAggregation::L1 => 1,
            PdcchAggregation::L2 => 2,
            PdcchAggregation::L4 => 4,
            PdcchAggregation::L8 => 8,
        }
    }

    pub const fn location_count(&self) -> usize {
        match self {
            PdcchAggregation::L1 => 6,
            PdcchAggregation::L2 => 6,
            PdcchAggregation::L4 => 2,
            PdcchAggregation::L8 => 2,
        }
    }

    pub const fn common_location_count(&self) -> usize {
        match self {
            PdcchAggregation::L1 => 0,
            PdcchAggregation::L2 => 0,
            PdcchAggregation::L4 => 4,
            PdcchAggregation::L8 => 2,
        }
    }

    pub const fn index(&self) -> usize {
        (*self as u8) as usize
    }

    pub const fn list() -> [PdcchAggregation; 4] {
        [
            PdcchAggregation::L1,
            PdcchAggregation::L2,
            PdcchAggregation::L4,
            PdcchAggregation::L8,
        ]
    }
}

pub type CceCountTable = [u8; defines::NUM_OF_CFI];

pub type CcePositions = BoundedVec<u8, 6>;
pub type CfiSearchSpace = [CcePositions; 4];
pub type SfSearchSpace = [CfiSearchSpace; defines::NUM_OF_CFI];
pub type SearchSpace = [SfSearchSpace; 10];

pub fn calculate_search_space(rnti: &Rnti, cce_count_table: &CceCountTable) -> SearchSpace {
    let mut search_space = SearchSpace::default();

    let mut y_k: u32 = rnti.0 as u32;
    const A: u32 = 39827;
    const D: u32 = 65537;

    for sf in 0..10 {
        for cfi in Cfi::list() {
            let cce_count = cce_count_table[cfi.index()]; // N_CCE,k

            for aggregation_level in PdcchAggregation::list() {
                let cce_mod = cce_count as u32 / aggregation_level.size() as u32; // [N_CCE,k / L]
                if cce_mod == 0 {
                    continue;
                }

                for m in 0..aggregation_level.location_count() as u32 {
                    // As described in 36.213 9.1.1
                    y_k = (A * y_k) % D;
                    let start_cce = aggregation_level.size() as u32 * ((y_k + m) % cce_mod);
                    search_space[sf][cfi.index()][aggregation_level.index()]
                        .push(start_cce as u8)
                        .unwrap();
                }
            }
        }
    }

    search_space
}

// TODO: proper value and placement
pub const MAX_PDCCH: usize = 16;