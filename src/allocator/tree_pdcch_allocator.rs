use crate::defines;
use crate::signalling::structs::{AllocationError, AllocationResult};
use crate::types::bitset::BoundedBitset;
use crate::types::bounded_vec::BoundedVec;
use crate::types::cfi::Cfi;
use crate::types::interval::Interval;
use crate::types::rnti::Rnti;
use serde::{Deserialize, Serialize};
use std::array;

use structures;

pub type PdcchSched = PdcchSchedTree;

/// State for dci allocation
#[derive(Clone, Debug)]
pub struct PdcchSchedTree {
    pub current_cfi: Cfi,
    dci_index: u8,

    candidate_trees: [AllocationTree; defines::NUM_OF_CFI],
}

impl PdcchSchedTree {
    pub fn new(cce_table: CceCountTable) -> Self {
        PdcchSchedTree {
            current_cfi: Default::default(),
            dci_index: 0,
            candidate_trees: array::from_fn(|i| AllocationTree::new(cce_table[i])),
        }
    }

    pub fn new_tti(&mut self) {
        self.current_cfi = Cfi::One;
        self.dci_index = 0;
        for tree in self.candidate_trees.iter_mut() {
            tree.reset();
        }
    }

    pub fn increment_cfi(&mut self) -> AllocationResult {
        //      println!("incrementing cfi");
        //      println!("last mask: {:?}", self.candidate_trees[self.current_cfi.index()]);
        match self.current_cfi {
            Cfi::One => {
                self.current_cfi = Cfi::Two;
                Ok(())
            }
            Cfi::Two => {
                self.current_cfi = Cfi::Three;
                Ok(())
            }
            Cfi::Three => Err(AllocationError::NoCchSpace),
        }
    }

    pub fn allocate_dci(
        &mut self,
        aggregation_level: PdcchAggregation,
        search_space: &SfSearchSpace,
        rnti: Rnti,
    ) -> Result<u8, AllocationError> {
        let start_cfi = self.current_cfi;
        for cfi in start_cfi.iter() {
            let res = self.candidate_trees[cfi.index()].try_alloc(
                aggregation_level,
                &search_space[cfi.index()],
                rnti,
            );
            match res {
                Ok(_) => (),
                Err(_) => {
                    self.increment_cfi()?;
                    continue;
                }
            }
        }
        self.dci_index += 1;
        Ok(self.dci_index - 1)
    }

    pub fn get_allocs(&self) -> (BoundedVec<PdcchAlloc, MAX_PDCCH>, CceMask, Cfi) {
        let (allocs, mask) = self.candidate_trees[self.current_cfi.index()].get_allocs();
        (allocs, mask, self.current_cfi)
    }
}

pub type CceMask = BoundedBitset<{ defines::NUM_OF_CCES }>;

/// Tree of allocation candidates. Path in this tree represents valid set of allocations
#[derive(Clone, Debug)]
struct AllocationTree {
    cce_count: u8,
    alloc_count: usize,

    /// Vector of (parent index, allocation)
    allocation_buffer: BoundedVec<(Option<usize>, PdcchAlloc),  33_554_432>, // 2^25, works up to 10 DCI

    /// Range of indices for lowest tree layer
    last_layer: Interval<usize, { usize::MAX }>,
}

impl AllocationTree {
    fn new(cce_count: u8) -> AllocationTree {
        AllocationTree {
            cce_count,
            alloc_count: 0,
            allocation_buffer: BoundedVec::new(),
            last_layer: Interval::new(0, 0).unwrap(),
        }
    }

    fn reset(&mut self) {
        self.alloc_count = 0;
        self.allocation_buffer.clear();
        self.last_layer = Interval::new(0, 0).unwrap();
    }

    fn try_alloc(
        &mut self,
        aggregation_level: PdcchAggregation,
        search_space: &CfiSearchSpace,
        rnti: Rnti,
    ) -> Result<(), ()> {
        if self.alloc_count == 0 {
            // First alloc
            let _ = self.try_single_alloc(None, aggregation_level, search_space, rnti);
        } else {
            for i in self.last_layer.range() {
                let _ = self.try_single_alloc(Some(i), aggregation_level, search_space, rnti);
            }
        }

        if self.allocation_buffer.len() == self.last_layer.len {
            return Err(());
        }

        self.last_layer = Interval::new(self.last_layer.len, self.allocation_buffer.len()).unwrap();
        self.alloc_count += 1;
        Ok(())
    }

    fn try_single_alloc(
        &mut self,
        parent_idx: Option<usize>,
        aggregation_level: PdcchAggregation,
        search_space: &CfiSearchSpace,
        rnti: Rnti,
    ) -> Result<(), ()> {
        let cum_mask = match parent_idx {
            Some(index) => self
                .allocation_buffer
                .get(index)
                .unwrap()
                .1
                .total_mask
                .clone(),
            None => CceMask::new(self.cce_count as usize),
        };
        for &start_cce in search_space[aggregation_level as usize].iter() {
            // TODO: check for SR collision

            let mut alloc_mask = CceMask::new(self.cce_count as usize);
            alloc_mask.fill(start_cce as usize, aggregation_level.size(), true)?;
            if (alloc_mask & cum_mask).any() {
                continue;
            }

            let alloc = PdcchAlloc {
                aggregation_level,
                start_cce,
                rnti,
                mask: alloc_mask,
                total_mask: alloc_mask | cum_mask,
            };

            self.allocation_buffer
                .push((parent_idx, alloc))
                .map_err(|_| ())?;
        }
        Ok(())
    }

    fn get_allocs(&self) -> (BoundedVec<PdcchAlloc, MAX_PDCCH>, CceMask) {
        if self.alloc_count == 0 {
            return (BoundedVec::default(), CceMask::new(self.cce_count as usize));
        }
        //    println!("internal alloc count {}", self.alloc_count);

        let mask = self.allocation_buffer[self.last_layer.start]
            .1
            .clone()
            .total_mask
            .clone();
        let mut allocs = BoundedVec::new();
        let mut index = Some(self.last_layer.start);
        while index.is_some() {
            let (nindex, alloc) = self.allocation_buffer[index.unwrap()].clone();
            allocs.push(alloc).unwrap();
            index = nindex;
        }

        allocs.reverse();

        (allocs, mask)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PdcchAlloc {
    // Location data
    pub aggregation_level: PdcchAggregation,
    pub start_cce: u8,

    pub rnti: Rnti,

    mask: CceMask,
    total_mask: CceMask,
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::scheduler_config::SchedulerConfig;
    use crate::types::tti::TtiPoint;
    use crate::{init, resource_allocation};

    #[test]
    fn a() {
        let sched_cfg = SchedulerConfig::default();
        let cell_config = init::cell_cfg(&sched_cfg);
        let count_table = resource_allocation::calculate_cce_count_table(&cell_config);

        let mut s = PdcchSched::new(count_table);
        s.new_tti();

        let tti = TtiPoint::from(1);

        let rnti = Rnti(1);
        let search_space = calculate_search_space(&rnti, &count_table);

        s.allocate_dci(
            PdcchAggregation::L1,
            &search_space[tti.to_usize() % 10],
            rnti,
        )
        .unwrap();
        s.allocate_dci(
            PdcchAggregation::L1,
            &search_space[tti.to_usize() % 10],
            rnti,
        )
        .unwrap();
        s.allocate_dci(
            PdcchAggregation::L1,
            &search_space[tti.to_usize() % 10],
            rnti,
        )
        .unwrap();
        s.allocate_dci(
            PdcchAggregation::L1,
            &search_space[tti.to_usize() % 10],
            rnti,
        )
        .unwrap();
        /*

        s.allocate_dci(
            PdcchAggregation::L1,
            &search_space[tti.to_usize() % 10],
            rnti,
        )
        .unwrap();
        s.allocate_dci(
            PdcchAggregation::L1,
            &search_space[tti.to_usize() % 10],
            rnti,
        )
        .unwrap();*/
        // s.allocate_dci(PdcchAggregation::L1, &search_space[tti.to_usize() % 10], rnti).unwrap();
        // s.allocate_dci(PdcchAggregation::L1, &search_space[tti.to_usize() % 10], rnti).unwrap();

        // s.allocate_dci(PdcchAggregation::L1, &search_space[tti.to_usize() % 10], rnti).unwrap();
        // s.allocate_dci(PdcchAggregation::L1, &search_space[tti.to_usize() % 10], rnti).unwrap();

        let (allocs, mask, cfi) = s.get_allocs();
        println!("{:?}", allocs);
        println!("{}", mask);
        println!("{:?}", cfi);
        println!("{:?}", search_space[tti.to_usize() % 10]);
    }

    #[test]
    fn pdcch_allocation_and_search_space() {
        let sched_cfg = SchedulerConfig::default();
        let cell_config = init::cell_cfg(&sched_cfg);
        let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
        let mut s = PdcchSched::new(count_table);
        s.new_tti();

        let tti = TtiPoint::from(1);

        let rnti = Rnti(70);
        let search_space = calculate_search_space(&rnti, &count_table);

        let aggr_level = PdcchAggregation::L1;

        s.allocate_dci(aggr_level, &search_space[tti.to_usize() % 10], rnti)
            .unwrap();

        let (allocs, mask, cfi) = s.get_allocs();
        println!("{:?}", allocs);
        println!("{}", mask);
        println!("{:?}", cfi);

        // Check if rnti is allocated in PDCCH
        assert_eq!(allocs[0].rnti, rnti);

        // Check if allocation is in rnti's search space
        assert!(
            search_space[tti.to_usize()][cfi as usize][aggr_level as usize]
                .iter()
                .find(|cce| **cce == allocs[0].start_cce)
                .is_some()
        );
        assert!(
            search_space[tti.to_usize()][cfi as usize][aggr_level as usize]
                .iter()
                .find(|cce| **cce == allocs[0].start_cce)
                .is_some()
        );

        assert_eq!(allocs.len(), 1);
    }

    #[test]
    fn pdcch_search_space_full() {
        let sched_cfg = SchedulerConfig::default();
        let cell_config = init::cell_cfg(&sched_cfg);
        let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
        let mut s = PdcchSched::new(count_table);
        s.new_tti();

        let tti = TtiPoint::from(1);

        let aggr_level = PdcchAggregation::L1;

        let mut search_space = SearchSpace::default();

        for cfiss in search_space[1].iter_mut() {
            for cfi in Cfi::list() {
                cfiss[cfi.index()].push(0).unwrap();
                cfiss[cfi.index()].push(1).unwrap();
                cfiss[cfi.index()].push(2).unwrap();
                cfiss[cfi.index()].push(3).unwrap();
                cfiss[cfi.index()].push(4).unwrap();
                cfiss[cfi.index()].push(5).unwrap();
            }
            for cfi in Cfi::list() {
                cfiss[cfi.index()].push(0).unwrap();
                cfiss[cfi.index()].push(1).unwrap();
                cfiss[cfi.index()].push(2).unwrap();
                cfiss[cfi.index()].push(3).unwrap();
                cfiss[cfi.index()].push(4).unwrap();
                cfiss[cfi.index()].push(5).unwrap();
            }
        }

        for r in 70..76 {
            let rnti = Rnti(r);
            s.allocate_dci(aggr_level, &search_space[tti.sf_idx()], rnti)
                .unwrap();
            s.allocate_dci(aggr_level, &search_space[tti.sf_idx()], rnti)
                .unwrap();
        }

        let (allocs, mask, cfi) = s.get_allocs();
        println!("{:?}", allocs);
        println!("{}", mask);
        println!("{:?}", cfi);

        assert_eq!(allocs.len(), 6);

        let mut expected_mask = CceMask::new(mask.size());
        for i in 0..6 {
            expected_mask.set(i, true).unwrap();
        }
        assert_eq!(mask, expected_mask);

        let res = s
            .allocate_dci(aggr_level, &search_space[tti.sf_idx()], Rnti(80))
            .expect_err("Allocation with full mask should fail");
        let res = s
            .allocate_dci(aggr_level, &search_space[tti.sf_idx()], Rnti(80))
            .expect_err("Allocation with full mask should fail");
        assert!(matches!(res, AllocationError::NoCchSpace));
    }
}
