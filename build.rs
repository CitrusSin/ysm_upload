use std::path::Path;
use std::process::{Command, Output};

fn emit_cargo_warnings(output: &[u8]) {
    for line in String::from_utf8_lossy(output).lines() {
        if !line.trim().is_empty() {
            println!("cargo:warning={}", line);
        }
    }
}

fn run_pnpm(frontend_dir: &Path, args: &[&str]) -> std::io::Result<Output> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("pnpm");
        command
    } else {
        Command::new("pnpm")
    };

    command.args(args).current_dir(frontend_dir).output()
}

fn main() {
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/package.json");
    
    let frontend_dir = Path::new("frontend");
    
    if !frontend_dir.exists() {
        println!("cargo:warning=Frontend directory not found, skipping frontend build");
        return;
    }

    // 检测是否安装了 pnpm (Windows 下使用 cmd)
    let has_pnpm = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "pnpm", "--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    } else {
        Command::new("pnpm")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    };

    if !has_pnpm {
        println!("cargo:warning=pnpm not found, skipping frontend build");
        return;
    }

    println!("cargo:warning=Building frontend...");

    // Install Dependencies
    let install_status = run_pnpm(frontend_dir, &["install", "--ignore-scripts"]);

    match install_status {
        Ok(output) if output.status.success() => {
            emit_cargo_warnings(&output.stdout);
            emit_cargo_warnings(&output.stderr);
            println!("cargo:warning=Frontend dependencies installed");
        }
        Ok(output) => {
            emit_cargo_warnings(&output.stdout);
            emit_cargo_warnings(&output.stderr);
            println!("cargo:warning=Failed to install frontend dependencies: exit code {}", output.status);
            return;
        }
        Err(e) => {
            println!("cargo:warning=Failed to run pnpm install: {}", e);
            return;
        }
    }

    // 构建前端
    let build_status = run_pnpm(frontend_dir, &["run", "build"]);

    match build_status {
        Ok(output) if output.status.success() => {
            emit_cargo_warnings(&output.stdout);
            emit_cargo_warnings(&output.stderr);
            println!("cargo:warning=Frontend build completed successfully");
        }
        Ok(output) => {
            emit_cargo_warnings(&output.stdout);
            emit_cargo_warnings(&output.stderr);
            panic!("Frontend build failed with exit code {}", output.status);
        }
        Err(e) => {
            panic!("Failed to run frontend build: {}", e);
        }
    }
}
