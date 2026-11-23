use std::sync::Mutex;
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Job {
    Import,
    Export,
    Move,
}
#[derive(Default)]
pub struct LibraryJobs(Mutex<Vec<Job>>);
pub struct JobGuard<'a> {
    jobs: &'a LibraryJobs,
    job: Job,
}
impl LibraryJobs {
    pub fn moving(&self) -> bool {
        self.0.lock().unwrap().contains(&Job::Move)
    }
    pub fn begin(&self, job: Job) -> Result<JobGuard<'_>, String> {
        let mut jobs = self.0.lock().unwrap();
        if jobs.contains(&Job::Move) {
            return Err("正在移动仓库，请等它完成".into());
        }
        if job == Job::Move && !jobs.is_empty() {
            return Err("请等导入或导出完成后再移动仓库".into());
        }
        if jobs.contains(&job) {
            return Err(match job {
                Job::Import => "正在导入另一批表情，请等它完成",
                _ => "正在导出另一批表情，请等它完成",
            }
            .into());
        }
        jobs.push(job);
        Ok(JobGuard { jobs: self, job })
    }
}
impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.jobs.0.lock().unwrap().retain(|j| *j != self.job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusion_matrix_and_release() {
        let jobs = LibraryJobs::default();
        for a in [Job::Import, Job::Export, Job::Move] {
            let guard = jobs.begin(a).unwrap();
            for b in [Job::Import, Job::Export, Job::Move] {
                assert_eq!(
                    jobs.begin(b).is_ok(),
                    a != b && a != Job::Move && b != Job::Move
                )
            }
            drop(guard);
            assert!(jobs.begin(a).is_ok())
        }
    }
}
