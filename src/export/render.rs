//! Render a DOT graph to SVG through the system Graphviz `dot`.
//!
//! The crate deliberately avoids a Graphviz dependency: the binaries shell out
//! to the `dot` on PATH. This module centralizes the "pipe DOT to `dot -Tsvg`,
//! write the resulting SVG to a file" step so the binaries do not drift.
//!
//! Both `stv` (writes a `.dot` file first, then renders it) and `sts` (holds the
//! DOT in memory and prints it to stdout) funnel their SVG rendering through
//! the same availability check and `dot` invocation implemented here.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Outcome of asking the system `dot` to render a DOT source to `svg_path`.
#[derive(Debug, PartialEq)]
pub enum SvgOutcome {
    /// `dot` ran successfully and the SVG was written to the target path.
    Generated,
    /// No `dot` on PATH. Nothing was written; the caller should point the user
    /// at installing Graphviz.
    GraphvizNotInstalled,
    /// `dot` was found but could not be rendered: it failed to launch, exited
    /// non-zero, or the SVG could not be written to disk. The wrapped string
    /// describes the failure for the caller to report.
    Failed(String),
}

/// Render `dot_source` to `svg_path` by piping it to the system `dot -Tsvg`.
///
/// `dot` reads DOT from stdin when no input file is given and writes SVG to
/// stdout, so this pipes `dot_source` in and captures the SVG bytes. The result
/// is written to `svg_path` only on success. The availability of `dot` is
/// probed first with `dot -V`; an absent `dot` short-circuits to
/// [`SvgOutcome::GraphvizNotInstalled`] before any SVG path is touched, so a
/// machine without Graphviz writes nothing.
pub fn render_dot_to_svg(dot_source: &str, svg_path: &Path) -> SvgOutcome {
    match Command::new("dot").arg("-V").output() {
        Ok(_) => {}
        Err(_) => return SvgOutcome::GraphvizNotInstalled,
    }

    let mut child = match Command::new("dot")
        .arg("-Tsvg")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return SvgOutcome::Failed(format!("failed to spawn 'dot': {}", e)),
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(dot_source.as_bytes()) {
            return SvgOutcome::Failed(format!("failed to write DOT to 'dot': {}", e));
        }
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(e) => return SvgOutcome::Failed(format!("failed to read 'dot' output: {}", e)),
    };

    if !output.status.success() {
        return SvgOutcome::Failed(format!("'dot' exited with status {}", output.status));
    }

    if let Err(e) = std::fs::write(svg_path, &output.stdout) {
        return SvgOutcome::Failed(format!(
            "failed to write '{}': {}",
            svg_path.display(),
            e
        ));
    }

    SvgOutcome::Generated
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial valid DOT source — just enough structure for `dot` to accept
    /// and render without error.
    const SMOKE_DOT: &str = "digraph G { a -> b; }";

    #[test]
    fn render_dot_to_svg_writes_svg_when_graphviz_present() {
        // Graphviz may or may not be installed in the test environment; either
        // outcome is legitimate, so the test accepts both and only pins down the
        // positive case (a real SVG on disk) when `dot` is available.
        let svg = std::env::temp_dir().join("sig_topo_render_smoke.svg");

        match render_dot_to_svg(SMOKE_DOT, &svg) {
            SvgOutcome::Generated => {
                let bytes = std::fs::read(&svg).expect("SVG should be readable when Generated");
                assert!(
                    windows_eq(&bytes, b"<svg"),
                    "rendered SVG should contain '<svg', got:\n{}",
                    String::from_utf8_lossy(&bytes)
                );
            }
            SvgOutcome::GraphvizNotInstalled => {
                // No `dot` on PATH: nothing should have been written.
                assert!(
                    !svg.exists(),
                    "no SVG should be written when Graphviz is absent"
                );
            }
            SvgOutcome::Failed(msg) => {
                panic!("smoke DOT should render cleanly, got failure: {}", msg);
            }
        }

        // Leave the temp dir clean whether or not a file was created.
        if svg.exists() {
            std::fs::remove_file(&svg).expect("should remove temp SVG");
        }
    }

    #[test]
    fn render_dot_to_svg_reports_failed_on_unparseable_dot() {
        // `dot` is present (implied when this runs alongside the smoke test),
        // but feeding it garbage makes it exit non-zero rather than produce an
        // SVG. The helper should surface that as `Failed`, not write a file.
        let svg = std::env::temp_dir().join("sig_topo_render_bad.svg");

        if Command::new("dot").arg("-V").output().is_err() {
            // No `dot` at all: the failure path we want to exercise is
            // unreachable, and `GraphvizNotInstalled` is the correct outcome —
            // nothing to assert beyond "it didn't write a bogus file".
            assert!(!svg.exists());
            return;
        }

        assert!(
            matches!(
                render_dot_to_svg("this is not valid dot { ???", &svg),
                SvgOutcome::Failed(_)
            ),
            "unparseable DOT should surface as a Failed outcome, not write a file"
        );
        assert!(!svg.exists(), "no SVG should be written on render failure");
    }

    /// True if `bytes` contains the `needle` byte slice anywhere.
    fn windows_eq(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|w| w == needle)
    }
}
