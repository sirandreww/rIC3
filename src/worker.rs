use super::{
    activity::Activity,
    basic::{BasicShare, ProofObligation},
    frames::Frames,
    solver::{BlockResult, PdrSolver},
};
use crate::utils::relation::cube_subsume_init;
use logic_form::Cube;
use std::{
    collections::{BinaryHeap, VecDeque},
    sync::Arc,
};

pub struct PdrWorker {
    pub solvers: Vec<PdrSolver>,
    pub frames: Frames,
    pub share: Arc<BasicShare>,
    pub activity: Activity,
    pub cav23_activity: Activity,
}

impl PdrWorker {
    pub fn new(share: Arc<BasicShare>) -> Self {
        Self {
            solvers: Vec::new(),
            frames: Frames::new(),
            activity: Activity::new(&share.aig),
            cav23_activity: Activity::new(&share.aig),
            share,
        }
    }

    pub fn depth(&self) -> usize {
        self.solvers.len() - 1
    }

    pub fn new_frame(&mut self) {
        self.frames.new_frame();
        self.solvers
            .push(PdrSolver::new(self.share.clone(), self.solvers.len()));
    }

    pub fn blocked<'a>(&'a mut self, frame: usize, cube: &Cube) -> BlockResult<'a> {
        assert!(!cube_subsume_init(&self.share.init, cube));
        assert!(frame > 0);
        self.solvers[frame - 1].block_fetch(&self.frames);
        self.solvers[frame - 1].blocked(cube)
    }

    fn generalize(&mut self, frame: usize, cube: Cube, simple: bool) -> (usize, Cube) {
        let cube = self.mic(frame, cube, simple);
        for i in frame + 1..=self.depth() {
            if let BlockResult::No(_) = self.blocked(i, &cube) {
                return (i, cube);
            }
        }
        (self.depth() + 1, cube)
    }

    pub fn block(&mut self, frame: usize, cube: Cube) -> bool {
        let mut heap = BinaryHeap::new();
        let mut heap_num = vec![0; self.depth() + 1];
        heap.push(ProofObligation::new(frame, cube));
        heap_num[frame] += 1;
        while let Some(ProofObligation { frame, cube }) = heap.pop() {
            if frame == 0 {
                return false;
            }
            assert!(!cube_subsume_init(&self.share.init, &cube));
            if self.share.args.verbose {
                println!("{:?}", heap_num);
                self.statistic();
            }
            heap_num[frame] -= 1;
            if self.frames.trivial_contained(frame, &cube) {
                continue;
            }
            match self.blocked(frame, &cube) {
                BlockResult::Yes(conflict) => {
                    let conflict = conflict.get_conflict();
                    // if self.share.args.ctp {
                    // let similars = self.frames.similar(&conflict, frame);
                    // let mut ans = false;
                    // for similar in similars.iter() {
                    //     self.share.statistic.lock().unwrap().test_a += 1;
                    //     if self.try_block(frame, &similar, 5, false) {
                    //         self.share.statistic.lock().unwrap().test_c += 1;
                    //         ans = true;
                    //     }
                    // }
                    // if ans {
                    //     continue;
                    // }
                    // self.share.statistic.lock().unwrap().test_b += 1;
                    // }
                    let (frame, core) = self.generalize(frame, conflict, false);
                    if frame <= self.depth() {
                        heap.push(ProofObligation::new(frame, cube));
                        heap_num[frame] += 1;
                    }
                    self.add_cube(frame - 1, core);
                }
                BlockResult::No(model) => {
                    heap.push(ProofObligation::new(frame - 1, model.get_model()));
                    heap.push(ProofObligation::new(frame, cube));
                    heap_num[frame - 1] += 1;
                    heap_num[frame] += 1;
                }
            }
        }
        true
    }

    pub fn try_block(
        &mut self,
        frame: usize,
        cube: &Cube,
        mut max_try: usize,
        simple: bool,
    ) -> bool {
        loop {
            if max_try == 0 || frame == 0 || cube_subsume_init(&self.share.init, cube) {
                return false;
            }
            assert!(!cube_subsume_init(&self.share.init, cube));
            max_try -= 1;
            match self.blocked(frame, cube) {
                BlockResult::Yes(conflict) => {
                    let conflict = conflict.get_conflict();
                    let (frame, core) = self.generalize(frame, conflict, simple);
                    self.add_cube(frame - 1, core);
                    return true;
                }
                BlockResult::No(cex) => {
                    let cex = cex.get_model();
                    if !self.try_block(frame - 1, &cex, max_try, true) {
                        return false;
                    }
                }
            }
        }
    }

    pub fn start(&mut self) -> bool {
        loop {
            if let Some(cex) = self.solvers.last_mut().unwrap().get_bad() {
                if !self.block(self.depth(), cex) {
                    return false;
                }
            } else {
                return true;
            }
        }
    }

    pub fn propagate(&mut self) -> bool {
        for frame_idx in 1..self.depth() {
            let mut frame = VecDeque::from_iter(self.frames[frame_idx].iter().cloned());
            while let Some(cube) = frame.pop_front() {
                if self.frames.trivial_contained(frame_idx + 1, &cube) {
                    continue;
                }
                if let BlockResult::Yes(conflict) = self.blocked(frame_idx + 1, &cube) {
                    let conflict = conflict.get_conflict();
                    self.add_cube(frame_idx + 1, conflict);
                    if self.share.args.cav23 {
                        self.activity.pump_cube_activity(&cube);
                    }
                }
            }
            if self.frames[frame_idx].is_empty() {
                return true;
            }
        }
        false
    }
}