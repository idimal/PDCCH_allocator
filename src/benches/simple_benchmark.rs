use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use schedrs::scheduler_config::SchedulerConfig;
use schedrs::signalling::sched_pdcch::{
    calculate_search_space, CceCountTable, PdcchAggregation, PdcchSched, SearchSpace,
};
use schedrs::types::rnti::Rnti;
use schedrs::types::tti::TtiPoint;
use schedrs::{init, resource_allocation};

fn pdcch(
    tti: TtiPoint,
    precalculated_values: &Vec<(
        PdcchAggregation,
        [[[schedrs::types::bounded_vec::BoundedVec<u8, 6>; 4]; 3]; 10],
        Rnti,
    )>,
    s: &mut PdcchSched,
) {
    for idx in 0..precalculated_values.len() {
        let (aggregation_level, search_space, rnti) = &precalculated_values[idx];

        let _ = s.allocate_dci(*aggregation_level, &search_space[tti.to_usize() % 10], *rnti);
    }
}

fn criterion_benchmark(c: &mut Criterion) {

    let mut cell_config = init::cell_cfg(&SchedulerConfig::default());
    cell_config.nof_prb = 100;

    let count_table = resource_allocation::calculate_cce_count_table(&cell_config);

    let mut s = PdcchSched::new(count_table);
    s.new_tti();

    let mut tti = TtiPoint::from(1);
    /// randomly generated DCI parameters
    let data = [
        (PdcchAggregation::L1, Rnti(63107)),
        (PdcchAggregation::L1, Rnti(23953)),
        (PdcchAggregation::L1, Rnti(39217)),
        (PdcchAggregation::L1, Rnti(35996)),
        (PdcchAggregation::L2, Rnti(5904)),
        (PdcchAggregation::L1, Rnti(62997)),
        (PdcchAggregation::L1, Rnti(61268)),
        (PdcchAggregation::L2, Rnti(11764)),
        (PdcchAggregation::L1, Rnti(7454)),
        (PdcchAggregation::L2, Rnti(25017)),
        (PdcchAggregation::L4, Rnti(53714)),
        (PdcchAggregation::L1, Rnti(13186)),
        (PdcchAggregation::L1, Rnti(8054)),
        (PdcchAggregation::L2, Rnti(4131)),
        (PdcchAggregation::L1, Rnti(37901)),
        (PdcchAggregation::L1, Rnti(28249)),
    ];

    let mut precalculated_values = Vec::new();
    for idx in 0..data.len() {
        let (aggregation_level, rnti) = data[idx];

        let search_space = calculate_search_space(&rnti, &count_table);

        precalculated_values.push((aggregation_level, search_space, rnti));
    }

    c.bench_function("pdcch", |b| {
        b.iter(|| {
            tti += 1;
            s.new_tti();
            pdcch(tti, &precalculated_values, &mut s);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
