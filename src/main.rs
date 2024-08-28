mod allocator;

use rand::rngs::ThreadRng;
use rand::Rng;
use std::time::Instant;
use allocator::tree_pdcch_allocator::PdcchSchedTree;
use clap::{arg, Parser};


#[derive(Parser, Debug)]
struct Args {
    // Number of test
    #[arg(short, long)]
    test: u8,

    // PRB count
    #[arg(short, long)]
    prb: u8,
}
/// to run: cargo run -r -- --test N --prb M (6, 100 and several others)
pub fn main() {
    let args = Args::parse();

    match args.test {
        1 => pdcch_allocate_test(args.prb),
        2 => pdcch_cnt_free_cce_test(args.prb),
        3 => pdcch_time_test(args.prb),
        0 => test(args.prb),
        _ => println!("Incorrect input"),
    }
}

fn print_searsc_spaces(rnti: Rnti) {
    let mut cell_config = init::cell_cfg(&SchedulerConfig::default());
    cell_config.nof_prb = 100;

    let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
    let rnti = Rnti(*rnti);

    let search_space = calculate_search_space(&rnti, &count_table);

    let tti = 1;
    let cfi = 0;

    for aggregation_level in 0..4 {
        let mut alloc_mask = CceMask::new(count_table[cfi] as usize);
        for start_cce in &search_space[tti][cfi][aggregation_level] {
            //  println!("mask size {}, start cce {}, len {}", alloc_mask.size(), start_cce, 2_u32.pow(aggregation_level as u32));
            alloc_mask
                .fill(
                    *start_cce as usize,
                    2_u32.pow(aggregation_level as u32) as usize,
                    true,
                )
                .unwrap();
        }
        println!("{}", alloc_mask);
    }
}

fn test(prb_count: u8) {
    let mut cell_config = init::cell_cfg(&SchedulerConfig::default());
    cell_config.nof_prb = prb_count;

    let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
    let mut s = PdcchSched::new(count_table);

    let data = [
        (PdcchAggregation::L1, Rnti(10249)),
        (PdcchAggregation::L1, Rnti(7386)),
        (PdcchAggregation::L1, Rnti(45236)),
        (PdcchAggregation::L4, Rnti(18567)),
        // (PdcchAggregation::L1, Rnti(38284)),
        // (PdcchAggregation::L1, Rnti(61138)),
        // (PdcchAggregation::L1, Rnti(55507)),
        // (PdcchAggregation::L1, Rnti(57099)),
        // (PdcchAggregation::L1, Rnti(2409)),
        // (PdcchAggregation::L2, Rnti(1403)),
    ];

    let dci_count = data.len();

    s.new_tti();
    let tti = TtiPoint::from(1);

    for i in 0..dci_count {
        let (aggregation_level, rnti) = data[i];
        let search_space = calculate_search_space(&rnti, &count_table);

        let _ = s.allocate_dci(aggregation_level, &search_space[tti.to_usize() % 10], rnti);
    }
    let (allocs, mask, cfi) = s.get_allocs();

    println!("{:?} \n", allocs);
    println!("{} \n", mask);
    println!("{:?}", cfi);
}

/// Dependence of the number of allocated DCI on the number of requested
fn pdcch_allocate_test(prb_count: u8) {
    let mut rng = rand::thread_rng();

    let mut cell_config = init::cell_cfg(&SchedulerConfig::default());
    cell_config.nof_prb = prb_count;

    let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
    let mut s = PdcchSched::new(count_table);

    for dci_count in 1..=20 {
        let mut ok_count: u64 = 0;

        for _ in 0..1_000 {
            s.new_tti();
            let tti = TtiPoint::from(rng.gen::<u16>());

            for _ in 0..dci_count {
                let rnti = Rnti(rng.gen());
                let search_space = calculate_search_space(&rnti, &count_table);

                let aggregation_level = gen_aggregation_level(&mut rng);

                let res =
                    s.allocate_dci(aggregation_level, &search_space[tti.to_usize() % 10], rnti);
                if res.is_ok() {
                    ok_count += 1;
                }
            }
        }
        let f = format!("{};{}", dci_count, ok_count as f64 / 1_000.0).replace(".", ",");
        println!("{}", f);
    }
}

/// Dependence of the number of unoccupied CCE on the number of requested DCI
fn pdcch_cnt_free_cce_test(prb_count: u8) {
    let mut rng = rand::thread_rng();

    let mut cell_config = init::cell_cfg(&SchedulerConfig::default());
    cell_config.nof_prb = prb_count;
    let max_dci_cnt = match prb_count {
        6 => 6,
        _ => 8,
    };

    let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
    let mut s = PdcchSched::new(count_table);

    for dci_count in 1..=max_dci_cnt {
        let mut counter = 0;
        let mut free_cce = 0;

        while counter < 1000 {
            s.new_tti();

            let tti = TtiPoint::from(rng.gen::<u16>());
            let mut ok_count = 0;

            for _ in 0..dci_count {
                let rnti = Rnti(rng.gen());
                let search_space = calculate_search_space(&rnti, &count_table);

                let aggregation_level = gen_aggregation_level(&mut rng);

                let res =
                    s.allocate_dci(aggregation_level, &search_space[tti.to_usize() % 10], rnti);
                if res.is_ok() {
                    ok_count += 1;
                }
            }

            if ok_count == dci_count {
                counter += 1;
                let (_, mask, _) = s.get_allocs();
                free_cce += mask.size() - mask.count();
            }
        }
        let f = format!("{};{}", dci_count, free_cce as f64 / 1_000.0).replace(".", ",");
        println!("{}", f);
    }
}

/// Dependence of operating time on the number of requested DCI
fn pdcch_time_test(prb_count: u8) {
    let mut rng = rand::thread_rng();

    let mut cell_config = init::cell_cfg(&SchedulerConfig::default());
    cell_config.nof_prb = prb_count;

    let count_table = resource_allocation::calculate_cce_count_table(&cell_config);
    let mut s = PdcchSched::new(count_table);

    let mut precalculated_values = Vec::new();
    for _ in 0..64_000 {
        let rnti = Rnti(rng.gen());

        let search_space = calculate_search_space(&rnti, &count_table);

        let aggregation_level = gen_aggregation_level(&mut rng);

        precalculated_values.push((aggregation_level, search_space, rnti));
    }

    let mut i = 0;
    for dci_count in 1..=8 {
        let start_time = Instant::now();

        for _ in 0..1_000 {
            s.new_tti();
            let tti = TtiPoint::from(rng.gen::<u16>());

            for _ in 0..dci_count {
                let _ = s.allocate_dci(
                    precalculated_values[i].0,
                    &precalculated_values[i].1[tti.to_usize() % 10],
                    precalculated_values[i].2,
                );
                i += 1;
            }
        }
        let duration = start_time.elapsed();

        let f = format!("{};{:?}", dci_count, duration / 1_000).replace(".", ",");
        println!("{}", f);
    }
}

fn gen_aggregation_level(rng: &mut ThreadRng) -> PdcchAggregation {
    let p: f64 = rng.gen();
    let aggregation_level = if p < 0.6 {
        PdcchAggregation::L1
    } else if p < 0.8 {
        PdcchAggregation::L2
    } else if p < 0.95 {
        PdcchAggregation::L4
    } else {
        PdcchAggregation::L8
    };
    aggregation_level
}
