use crate::{
    utils::relation::{cube_subsume, cube_subsume_init},
    worker::PdrWorker,
};
use logic_form::Cube;
use std::{
    fmt::Debug,
    mem::take,
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub struct Frames {
    pub frames: Vec<Vec<Cube>>,
}

impl Frames {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    pub fn new_frame(&mut self) {
        self.frames.push(Vec::new());
    }

    pub fn trivial_contained(&self, frame: usize, cube: &Cube) -> bool {
        for i in frame..self.frames.len() {
            for c in self.frames[i].iter() {
                if cube_subsume(c, cube) {
                    return true;
                }
            }
        }
        false
    }

    pub fn statistic(&self) {
        for frame in self.frames.iter() {
            print!("{} ", frame.len());
        }
        println!();
    }

    pub fn similar(&self, cube: &Cube, frame: usize) -> Vec<Cube> {
        let mut cube = cube.clone();
        cube.sort_by_key(|l| l.var());
        let mut res = Vec::new();
        if frame == 1 {
            return res;
        }
        for c in self.frames[frame - 1].iter() {
            if cube_subsume(c, &cube) {
                res.push(c.clone());
            }
        }
        res
    }
}

impl Debug for Frames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.frames.fmt(f)
    }
}

impl Deref for Frames {
    type Target = Vec<Vec<Cube>>;

    fn deref(&self) -> &Self::Target {
        &self.frames
    }
}

impl DerefMut for Frames {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.frames
    }
}

impl PdrWorker {
    pub fn add_cube(&mut self, frame: usize, mut cube: Cube) {
        cube.sort_by_key(|x| x.var());
        let begin = if frame == 0 {
            assert!(self.frames.len() == 1);
            0
        } else {
            if self.frames.trivial_contained(frame, &cube) {
                return;
            }
            assert!(!cube_subsume_init(&self.share.init, &cube));
            let mut begin = 1;
            for i in 1..=frame {
                let cubes = take(&mut self.frames[i]);
                for c in cubes {
                    if cube_subsume(&c, &cube) {
                        begin = i + 1;
                    }
                    if !cube_subsume(&cube, &c) {
                        self.frames[i].push(c);
                    }
                }
            }
            begin
        };
        self.frames[frame].push(cube.clone());
        let clause = Arc::new(!cube);
        for i in begin..=frame {
            self.solvers[i].add_clause(&clause);
        }
    }
}