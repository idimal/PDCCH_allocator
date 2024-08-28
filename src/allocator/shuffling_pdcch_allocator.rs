use crate::defines;
use crate::signalling::structs::{AllocationError, AllocationResult};
use crate::types::bitset::BoundedBitset;
use crate::types::bounded_vec::BoundedVec;
use crate::types::cfi::Cfi;
use crate::types::rnti::Rnti;
use rand::Rng;
use std::array;

use super::sched_pdcch::{
    CceCountTable, CfiSearchSpace, PdcchAggregation, SfSearchSpace, MAX_PDCCH,
};

#[derive(Clone, Debug)]
pub struct PdcchSchedShuffling {
    pub current_cfi: Cfi,
    dci_index: u8,

    allocation_buffer: [ShufflingAllocation; defines::NUM_OF_CFI],
}

impl PdcchSchedShuffling {
    pub fn new(cce_table: CceCountTable) -> Self {
        PdcchSchedShuffling {
            current_cfi: Default::default(),
            dci_index: 0,
            allocation_buffer: array::from_fn(|i| ShufflingAllocation::new(cce_table[i])),
        }
    }

    pub fn new_tti(&mut self) {
        self.current_cfi = Cfi::One;
        self.dci_index = 0;
        for alloc_buf in self.allocation_buffer.iter_mut() {
            alloc_buf.reset();
        }
    }

    pub fn increment_cfi(&mut self) -> AllocationResult {
        //        println!("incrementing cfi");
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
            let res = self.allocation_buffer[cfi.index()].try_alloc(
                aggregation_level,
                &search_space[cfi.index()],
                rnti,
            );
            match res {
                Ok(()) => (),
                Err(()) => {
                    self.increment_cfi()?;
                    continue;
                }
            }
        }

        self.dci_index += 1;
        Ok(self.dci_index - 1)
    }

    pub fn get_allocs(&self) -> (BoundedVec<PdcchAllocation, MAX_PDCCH>, CceMask, Cfi) {
        let (allocation_buffer, total_mask) =
            self.allocation_buffer[self.current_cfi.index()].get_allocs();
        (allocation_buffer, total_mask, self.current_cfi)
    }
}

pub type CceMask = BoundedBitset<{ defines::NUM_OF_CCES }>;

#[derive(Clone, Debug)]
struct ShufflingAllocation {
    cce_count: u8,

    /// Vector of allocations
    allocation_buffer: BoundedVec<PdcchAllocation, 16>,
    search_space_buffer: BoundedVec<CfiSearchSpace, 16>,
    total_mask: CceMask,
}

impl ShufflingAllocation {
    fn new(cce_count: u8) -> ShufflingAllocation {
        ShufflingAllocation {
            cce_count,
            allocation_buffer: BoundedVec::new(),
            search_space_buffer: BoundedVec::new(),
            total_mask: CceMask::new(cce_count as usize),
        }
    }

    fn reset(&mut self) {
        self.allocation_buffer.clear();
        self.search_space_buffer.clear();
        self.total_mask = CceMask::new(self.cce_count as usize);
    }

    fn try_alloc(
        &mut self,
        aggregation_level: PdcchAggregation,
        search_space: &CfiSearchSpace,
        rnti: Rnti,
    ) -> Result<(), ()> {
        let search_space_len = search_space[aggregation_level as usize].len();

        if search_space_len != 0 {

            let start_cce_idx = rand::thread_rng().gen_range(0..search_space_len);

            for cce_idx in start_cce_idx..(start_cce_idx + search_space_len) {
                let mut alloc_mask = CceMask::new(self.cce_count as usize);
                alloc_mask.fill(
                    search_space[aggregation_level as usize][cce_idx % search_space_len] as usize,
                    aggregation_level.size(),
                    true,
                )?;
                if (alloc_mask & self.total_mask).any() {
                    continue;
                } else {
                    let start_cce = search_space[aggregation_level as usize][cce_idx % search_space_len];
                    let alloc = PdcchAllocation {
                        aggregation_level,
                        start_cce,
                        rnti,
                        mask: alloc_mask,
                    };
                    self.total_mask = alloc_mask | self.total_mask;
                    self.allocation_buffer.push(alloc).map_err(|_| ())?;
                    self.search_space_buffer
                    .push(search_space.clone())
                    .map_err(|_| ())?;
                    return Ok(());
                }
            }
        }
        self.shuffle(aggregation_level, search_space, rnti)
    }

    fn shuffle(
        &mut self,
        aggregation_level: PdcchAggregation,
        search_space: &CfiSearchSpace,
        rnti: Rnti,
    ) -> Result<(), ()> {
        for &start_cce in search_space[aggregation_level as usize].iter() {
            let mut alloc_mask = CceMask::new(self.cce_count as usize);
            alloc_mask.fill(start_cce as usize, aggregation_level.size(), true)?;
            let mut flag = false;
            for idx in 0..self.allocation_buffer.len() {
                if (alloc_mask & self.allocation_buffer[idx].mask).any() {
                    for &some_cce in self.search_space_buffer[idx][self.allocation_buffer[idx].aggregation_level as usize].iter() {
                        let mut temporary_mask = CceMask::new(self.cce_count as usize);
                        temporary_mask.fill(some_cce as usize, self.allocation_buffer[idx].aggregation_level.size(), true)?;

                        if (temporary_mask & alloc_mask).any() | (temporary_mask & self.total_mask).any() {
                            flag = false;
                            continue;
                        } else {
                            self.allocation_buffer[idx].mask = temporary_mask;
                            self.allocation_buffer[idx].start_cce = some_cce;
                            self.total_mask = CceMask::new(self.cce_count as usize);
                            for i in 0..self.allocation_buffer.len() {
                                self.total_mask = self.total_mask | self.allocation_buffer[i].mask;
                            }
                            flag = true;
                            break;
                        }
                    }
                } else {
                    continue;
                }
            }
            if flag {
                let alloc = PdcchAllocation {
                    aggregation_level,
                    start_cce,
                    rnti,
                    mask: alloc_mask,
                };
                self.total_mask = alloc_mask | self.total_mask;
                self.allocation_buffer.push(alloc).map_err(|_| ())?;
                self.search_space_buffer
                .push(search_space.clone())
                .map_err(|_| ())?;
                return Ok(());
            }
        }

        Err(())
    }

    fn get_allocs(&self) -> (BoundedVec<PdcchAllocation, MAX_PDCCH>, CceMask) {
        (self.allocation_buffer.clone(), self.total_mask)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PdcchAllocation {
    // Location data
    pub aggregation_level: PdcchAggregation,
    pub start_cce: u8,
    pub rnti: Rnti,
    mask: CceMask,
}
