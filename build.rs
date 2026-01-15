use std::process::Command;
use std::path::Path;

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
            .is_ok()
    } else {
        Command::new("pnpm")
            .arg("--version")
            .output()
            .is_ok()
    };

    if !has_pnpm {
        println!("cargo:warning=pnpm not found, skipping frontend build");
        return;
    }

    println!("cargo:warning=Building frontend...");

    // 安装依赖
    let install_status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "pnpm", "install"])
            .current_dir(frontend_dir)
            .status()
    } else {
        Command::new("pnpm")
            .arg("install")
            .current_dir(frontend_dir)
            .status()
    };

    match install_status {
        Ok(status) if status.success() => {
            println!("cargo:warning=Frontend dependencies installed");
        }
        Ok(status) => {
            println!("cargo:warning=Failed to install frontend dependencies: exit code {}", status);
            return;
        }
        Err(e) => {
            println!("cargo:warning=Failed to run pnpm install: {}", e);
            return;
        }
    }

    // 构建前端
    let build_status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "pnpm", "run", "build"])
            .current_dir(frontend_dir)
            .status()
    } else {
        Command::new("pnpm")
            .args(&["run", "build"])
            .current_dir(frontend_dir)
            .status()
    };

    match build_status {
        Ok(status) if status.success() => {
            println!("cargo:warning=Frontend build completed successfully");
        }
        Ok(status) => {
            panic!("Frontend build failed with exit code {}", status);
        }
        Err(e) => {
            panic!("Failed to run frontend build: {}", e);
        }
    }
}
