use std::collections::VecDeque;

use crate::InitConfig;
use crate::unit::{Unit, UnitId, UnitKind, UnitStore};

pub struct Scheduler {
    pending: VecDeque<Job>,
}

struct Job {
    unit: UnitId,
    kind: JobKind,
}

enum JobKind {
    Start,
}

impl Scheduler {
    pub fn new() -> Scheduler {
        Scheduler {
            pending: VecDeque::new(),
        }
    }

    pub fn schedule_start_and_report_errors(
        &mut self,
        unit_store: &mut UnitStore,
        unit_id: UnitId,
    ) {
        let mut errors = vec![];
        self.schedule_start(unit_store, unit_id, &mut errors);
        for error in errors {
            eprintln!("init: {error}");
        }
    }

    pub fn schedule_start(
        &mut self,
        unit_store: &mut UnitStore,
        unit_id: UnitId,
        errors: &mut Vec<String>,
    ) {
        let loaded_units = unit_store.load_units(unit_id.clone(), errors);
        for unit_id in loaded_units {
            if !unit_store.unit(&unit_id).conditions_met() {
                continue;
            }

            self.pending.push_back(Job {
                unit: unit_id,
                kind: JobKind::Start,
            });
        }
    }

    pub fn step(&mut self, unit_store: &mut UnitStore, init_config: &mut InitConfig) {
        'a: loop {
            let Some(job) = self.pending.pop_front() else {
                return;
            };

            match job.kind {
                JobKind::Start => {
                    let unit = unit_store.unit_mut(&job.unit);

                    for dep in &unit.info.requires_weak {
                        for pending_job in &self.pending {
                            if &pending_job.unit == dep {
                                self.pending.push_back(job);
                                continue 'a;
                            }
                        }
                    }

                    run(unit, init_config);
                }
            }
        }
    }
}

fn run(unit: &mut Unit, config: &mut InitConfig) {
    match &unit.kind {
        UnitKind::LegacyScript { script } => {
            for cmd in script.clone() {
                if config.log_debug {
                    eprintln!("init: running: {cmd:?}");
                }
                cmd.run(config);
            }
        }
        UnitKind::Service { service } => {
            if config.skip_cmd.contains(&service.cmd) {
                eprintln!("Skipping '{} {}'", service.cmd, service.args.join(" "));
                return;
            }
            if config.log_debug {
                eprintln!(
                    "Starting {} ({})",
                    unit.info.description.as_ref().unwrap_or(&unit.id.0),
                    service.cmd,
                );
            }
            service.spawn(&config.envs);
        }
        UnitKind::Target {} => {
            if config.log_debug {
                eprintln!(
                    "Reached target {}",
                    unit.info.description.as_ref().unwrap_or(&unit.id.0),
                );
            }
        }
    }
}
