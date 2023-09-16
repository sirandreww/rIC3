#![feature(assert_matches, is_sorted, get_mut_unchecked)]

mod activity;
mod basic;
mod command;
mod frames;
mod mic;
mod solver;
mod statistic;
mod utils;
mod verify;
mod worker;

pub use command::Args;
use pic3::LemmaSharer;

use crate::utils::state_transform::StateTransform;
use crate::{basic::BasicShare, statistic::Statistic, worker::Ic3Worker};
use aig::Aig;
use logic_form::{Cube, Lit};
use std::collections::HashMap;
use std::{
    mem::take,
    sync::{Arc, Mutex},
    thread::spawn,
    time::Instant,
};

pub struct Ic3 {
    workers: Vec<Ic3Worker>,
    pub share: Arc<BasicShare>,
}

impl Ic3 {
    pub fn new_frame(&mut self) {
        for worker in self.workers.iter_mut() {
            worker.new_frame()
        }
    }
}

impl Ic3 {
    pub fn new(args: Args, sharer: Option<LemmaSharer>) -> Self {
        let aig = Aig::from_file(args.model.as_ref().unwrap()).unwrap();
        let transition_cnf = aig.get_cnf();
        let mut init = HashMap::new();
        for l in aig.latch_init_cube().to_cube() {
            init.insert(l.var(), l.polarity());
        }
        let state_transform = StateTransform::new(&aig);
        let share = Arc::new(BasicShare {
            aig,
            transition_cnf,
            state_transform,
            args,
            init,
            statistic: Mutex::new(Statistic::default()),
        });
        let mut workers = vec![Ic3Worker::new(share.clone(), sharer)];
        for worker in workers.iter_mut() {
            worker.new_frame()
        }
        let mut res = Self { workers, share };
        for l in res.share.aig.latchs.iter() {
            if let Some(init) = l.init {
                let cube = Cube::from([Lit::new(l.input.into(), !init)]);
                for worker in res.workers.iter_mut() {
                    worker.add_cube(0, cube.clone())
                }
            }
        }
        res
    }

    pub fn check(&mut self) -> bool {
        if self.workers[0].solvers[0].get_bad().is_some() {
            return false;
        }
        self.new_frame();
        loop {
            let mut joins = Vec::new();
            let workers = take(&mut self.workers);
            let start = Instant::now();
            for mut worker in workers.into_iter() {
                joins.push(spawn(move || {
                    let res = worker.start();
                    (worker, res)
                }));
            }
            for join in joins {
                let (worker, res) = join.join().unwrap();
                if !res {
                    worker.statistic();
                    return false;
                }
                self.workers.push(worker)
            }
            let blocked_time = start.elapsed();
            println!(
                "[{}:{}] frame: {}, time: {:?}",
                file!(),
                line!(),
                self.workers[0].depth(),
                blocked_time,
            );
            self.share.statistic.lock().unwrap().overall_block_time += blocked_time;
            self.statistic();
            self.new_frame();
            let start = Instant::now();
            let propagate = self.workers[0].propagate();
            self.share.statistic.lock().unwrap().overall_propagate_time += start.elapsed();
            if propagate {
                self.statistic();
                assert!(self.workers[0].verify());
                return true;
            }
        }
    }
}