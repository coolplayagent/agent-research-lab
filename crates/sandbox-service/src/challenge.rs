use anyhow::{Result, ensure};
use std::path::Path;
pub(super) struct Challenge {
    expected: Vec<u8>,
}
impl Challenge {
    pub fn prepare(work: &Path, input: &mut String) -> Result<Self> {
        let entropy = tempfile::Builder::new()
            .prefix("nonce-")
            .tempfile_in(work)?;
        let nonce = storage::digest(
            format!("{:?}-{:?}", entropy.path(), std::time::SystemTime::now()).as_bytes(),
        );
        let expected = format!("outcome-hub-verified:{nonce}\n").into_bytes();
        std::fs::write(work.join("binding-input.txt"), &expected)?;
        let mut request: service_api::AgentInput = serde_json::from_str(input)?;
        request.objective = "Read binding-input.txt in the current workspace and write its exact bytes to binding-result.txt. Use your file tools or a shell. Do not print the contents. Do not modify the source file.".into();
        request.acceptance = "The host must independently verify the result file. Complete this single task without delegation or further model tasks.".into();
        request.instruction =
            "Return the required JSON result only after the file is written.".into();
        *input = serde_json::to_string(&request)?;
        Ok(Self { expected })
    }
    pub fn verify(&self, work: &Path) -> Result<()> {
        let path = work.join("binding-result.txt");
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == self.expected.len() as u64,
            "binding result must be a regular file with the expected size"
        );
        ensure!(
            std::fs::read(path)? == self.expected,
            "binding result does not match the host challenge"
        );
        Ok(())
    }
}
#[cfg(test)]
#[path = "challenge_tests.rs"]
mod tests;
