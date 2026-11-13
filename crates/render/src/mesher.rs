//! Worker threads that mesh chunks off the render thread, as vanilla 1.8.9 `ChunkRenderDispatcher`:
//! a fixed pool of 2 workers and a bounded number of outstanding tasks. The render thread uploads the results.

use crate::assets::Atlas;
use crate::mesh::{mesh_chunk, Meshes, Neighborhood};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// `new ChunkRenderWorker` is created twice in `ChunkRenderDispatcher`'s constructor.
const WORKERS: usize = 2;

/// Block data of one chunk, shared between the renderer and the jobs that read it.
pub type SharedBlocks = Arc<Vec<u16>>;

/// Biome ids of one chunk, index `z * 16 + x`.
pub type SharedBiomes = Arc<[u8; 256]>;

/// A snapshot of the chunk and its eight neighbours, ordered as `Neighborhood::neighbors`.
pub struct Job {
    pub key: (i32, i32),
    /// Tells a result from the job that is still wanted for `key`.
    pub generation: u64,
    pub center: SharedBlocks,
    pub neighbors: [Option<SharedBlocks>; 8],
    /// Biomes of the chunk and its neighbours, as `Neighborhood::biomes`.
    pub biomes: [Option<SharedBiomes>; 9],
}

pub struct Done {
    pub key: (i32, i32),
    pub generation: u64,
    pub meshes: Meshes,
}

impl Job {
    /// Mesh on the calling thread.
    pub fn run(self, atlas: &Atlas) -> Done {
        let hood = Neighborhood {
            origin: self.key,
            center: &self.center,
            neighbors: std::array::from_fn(|i| self.neighbors[i].as_deref().map(Vec::as_slice)),
            biomes: std::array::from_fn(|i| self.biomes[i].as_deref()),
        };
        Done {
            key: self.key,
            generation: self.generation,
            meshes: mesh_chunk(&hood, atlas),
        }
    }
}

pub struct Mesher {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

impl Mesher {
    pub fn new(atlas: Arc<Atlas>) -> Mesher {
        let (jobs, job_rx) = channel::<Job>();
        let (done_tx, done) = channel();
        let job_rx = Arc::new(Mutex::new(job_rx));
        for i in 0..WORKERS {
            let (job_rx, done_tx, atlas) = (job_rx.clone(), done_tx.clone(), atlas.clone());
            std::thread::Builder::new()
                .name(format!("Chunk Batcher {i}"))
                .spawn(move || loop {
                    // The lock is released before meshing: only the wait for a job is serialized.
                    let Ok(job) = job_rx.lock().unwrap().recv() else {
                        return;
                    };
                    if done_tx.send(job.run(&atlas)).is_err() {
                        return;
                    }
                })
                .expect("spawn chunk mesh worker");
        }
        Mesher { jobs, done }
    }

    pub fn submit(&self, job: Job) {
        self.jobs.send(job).expect("chunk mesh workers exited");
    }

    pub fn try_recv(&self) -> Option<Done> {
        self.done.try_recv().ok()
    }

    pub fn recv(&self) -> Done {
        self.done.recv().expect("chunk mesh workers exited")
    }
}
