//! Controlled synthetic workloads used only by the Milestone 1 Windows
//! acceptance harness. Every mode has a finite duration and no external side
//! effects beyond an explicitly supplied scratch path in the I/O modes.

use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    process::Command,
    thread,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
use std::{io, os::unix::process::CommandExt};

fn main() {
    let mut args = env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    match mode.as_str() {
        "memory-ramp" => memory_ramp(),
        "memory-spike" => memory_spike(),
        "cpu-single" => spin_cpu(Duration::from_millis(1_800)),
        "cpu-multi" => spin_cpu_multi(),
        "sequential-read" => {
            sequential_read(&args.next().expect("W9 source path"));
            thread::sleep(Duration::from_millis(1_200));
        }
        "sequential-write" => {
            sequential_write(&args.next().expect("W10 destination path"));
            thread::sleep(Duration::from_millis(1_200));
        }
        "target-abort" => std::process::abort(),
        "long-hold" => thread::sleep(Duration::from_secs(7)),
        "cpu-child" => spin_cpu(Duration::from_millis(800)),
        "child-cpu-then-exit" => {
            let status = Command::new(env::current_exe().expect("current workload executable"))
                .arg("cpu-child")
                .status()
                .expect("launch W7 child");
            assert!(status.success(), "W7 child status: {status}");
            // Keep the root in its Job after the child exits so later samples
            // must retain the child's cumulative Job CPU total.
            thread::sleep(Duration::from_millis(1_200));
        }
        "io-write" => sequential_write(&args.next().expect("W8 scratch path")),
        "child-tree" => {
            let status = Command::new(env::current_exe().expect("current workload executable"))
                .arg("tree-child")
                .status()
                .expect("launch W3 child");
            assert!(status.success(), "W3 child status: {status}");
        }
        "tree-child" => {
            let mut grandchild =
                Command::new(env::current_exe().expect("current workload executable"))
                    .arg("tree-hold")
                    .spawn()
                    .expect("launch W3 grandchild");
            thread::sleep(Duration::from_millis(1_400));
            assert!(grandchild.wait().expect("wait W3 grandchild").success());
        }
        "tree-hold" => thread::sleep(Duration::from_millis(1_600)),
        "root-exit-child-hold" => {
            let _child = Command::new(env::current_exe().expect("current workload executable"))
                .arg("standalone-child-hold")
                .spawn()
                .expect("launch A11 child");
            // Ensure the probe's initial and 500ms snapshots can retain it,
            // then exit the root while the child remains alive.
            thread::sleep(Duration::from_millis(650));
        }
        "standalone-child-hold" => thread::sleep(Duration::from_millis(1_400)),
        "short-lived-children" => {
            let mut children = (0..32)
                .map(|_| {
                    Command::new(env::current_exe().expect("current workload executable"))
                        .arg("short-hold")
                        .spawn()
                        .expect("launch W4 child")
                })
                .collect::<Vec<_>>();
            thread::sleep(Duration::from_millis(900));
            for child in &mut children {
                assert!(child.wait().expect("wait W4 child").success());
            }
            thread::sleep(Duration::from_millis(600));
        }
        "short-hold" => thread::sleep(Duration::from_millis(800)),
        "child-io-then-exit" => {
            let scratch = args.next().expect("W8 scratch path");
            let status = Command::new(env::current_exe().expect("current workload executable"))
                .args(["io-write", &scratch])
                .status()
                .expect("launch W8 child");
            assert!(status.success(), "W8 child status: {status}");
            // Under concurrent acceptance workloads the sampler can be delayed;
            // retain the root well past several absolute 500ms deadlines so
            // post-child-exit Job accounting has a deterministic observation window.
            thread::sleep(Duration::from_millis(2_200));
        }
        #[cfg(target_os = "linux")]
        "linux-ordinary-child" => linux_ordinary_child(&args.next().expect("child report path")),
        #[cfg(target_os = "linux")]
        "linux-grandchild" => linux_grandchild(&args.next().expect("child report path")),
        #[cfg(target_os = "linux")]
        "linux-grandchild-parent" => {
            linux_grandchild_parent(&args.next().expect("child report path"))
        }
        #[cfg(target_os = "linux")]
        "linux-child-exits-first" => {
            linux_child_exits_first(&args.next().expect("child report path"))
        }
        #[cfg(target_os = "linux")]
        "linux-root-exits-child-alive" => {
            linux_root_exits_child_alive(&args.next().expect("child report path"))
        }
        #[cfg(target_os = "linux")]
        "linux-child-new-session" => {
            linux_child_new_session(&args.next().expect("child report path"))
        }
        #[cfg(target_os = "linux")]
        "linux-hold-brief" => thread::sleep(Duration::from_millis(250)),
        #[cfg(target_os = "linux")]
        "linux-hold-long" => thread::sleep(Duration::from_secs(10)),
        #[cfg(target_os = "linux")]
        // Leave enough time for the probe to persist one live identity/sample,
        // then exit before its next 500ms cadence. An immediate exit races
        // identity acquisition on loaded hosted runners and cannot exercise the
        // intended pre-exit-sample contract.
        "linux-exit" => thread::sleep(Duration::from_millis(250)),
        #[cfg(target_os = "linux")]
        "linux-session-child" => linux_session_child(&args.next().expect("child report path")),
        _ => panic!("unknown synthetic workload mode: {mode}"),
    }
}

#[cfg(target_os = "linux")]
fn linux_child(mode: &str, report: &str) -> Command {
    let mut command = Command::new(env::current_exe().expect("current workload executable"));
    command.args([mode, report]);
    command
}

#[cfg(target_os = "linux")]
fn report_pid(report: &str, pid: u32) {
    fs::write(report, format!("{pid}\n")).expect("write fixture child PID");
}

#[cfg(target_os = "linux")]
fn linux_ordinary_child(report: &str) {
    let mut child = linux_child("linux-hold-brief", report)
        .spawn()
        .expect("launch ordinary Linux child");
    report_pid(report, child.id());
    assert!(child.wait().expect("wait ordinary Linux child").success());
}

#[cfg(target_os = "linux")]
fn linux_grandchild(report: &str) {
    let status = linux_child("linux-grandchild-parent", report)
        .status()
        .expect("launch Linux grandchild parent");
    assert!(status.success(), "Linux grandchild parent status: {status}");
}

#[cfg(target_os = "linux")]
fn linux_grandchild_parent(report: &str) {
    let mut grandchild = linux_child("linux-hold-brief", report)
        .spawn()
        .expect("launch Linux grandchild");
    fs::write(
        report,
        format!("{}\n{}\n", std::process::id(), grandchild.id()),
    )
    .expect("write fixture descendant PIDs");
    assert!(grandchild.wait().expect("wait Linux grandchild").success());
}

#[cfg(target_os = "linux")]
fn linux_child_exits_first(report: &str) {
    let mut child = linux_child("linux-exit", report)
        .spawn()
        .expect("launch exiting Linux child");
    report_pid(report, child.id());
    assert!(child.wait().expect("wait exiting Linux child").success());
    thread::sleep(Duration::from_millis(250));
}

#[cfg(target_os = "linux")]
fn linux_root_exits_child_alive(report: &str) {
    let child = linux_child("linux-hold-long", report)
        .spawn()
        .expect("launch surviving Linux child");
    report_pid(report, child.id());
    thread::sleep(Duration::from_millis(250));
}

#[cfg(target_os = "linux")]
fn linux_child_new_session(report: &str) {
    let mut command = linux_child("linux-session-child", report);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = command.spawn().expect("launch new-session Linux child");
    let status = child.wait().expect("wait new-session Linux child");
    assert!(status.success());
}

#[cfg(target_os = "linux")]
fn linux_session_child(report: &str) {
    unsafe {
        fs::write(
            report,
            format!(
                "{} {} {}\n",
                libc::getpid(),
                libc::getpgrp(),
                libc::getsid(0)
            ),
        )
        .expect("write new-session Linux child identity");
    }
    thread::sleep(Duration::from_millis(250));
}

/// W1: touch each allocation so Windows must reflect the sustained committed
/// private/working-set ramp in the target's raw process counters.
fn memory_ramp() {
    let mut blocks = Vec::new();
    for _ in 0..12 {
        blocks.push(vec![0xa5_u8; 2 * 1024 * 1024]);
        thread::sleep(Duration::from_millis(250));
    }
    std::hint::black_box(&blocks);
    thread::sleep(Duration::from_millis(1_200));
}

/// W2: preserve a short, touched allocation long enough to cross at least one
/// sampler deadline, then release it while the target keeps running.
fn memory_spike() {
    thread::sleep(Duration::from_millis(700));
    let spike = vec![0x5a_u8; 64 * 1024 * 1024];
    std::hint::black_box(&spike);
    thread::sleep(Duration::from_millis(900));
    drop(spike);
    thread::sleep(Duration::from_millis(900));
}

fn spin_cpu(duration: Duration) {
    let until = Instant::now() + duration;
    let mut value = 0_u64;
    while Instant::now() < until {
        value = value.wrapping_mul(6364136223846793005).wrapping_add(1);
        std::hint::black_box(value);
    }
}

/// W6: each reported logical processor receives a worker for the same bounded
/// interval. The parent joins every worker before target completion.
fn spin_cpu_multi() {
    let worker_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(2)
        .max(2);
    let workers = (0..worker_count)
        .map(|_| thread::spawn(|| spin_cpu(Duration::from_millis(1_800))))
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().expect("W6 worker must finish");
    }
}

fn sequential_read(path: &str) {
    let mut file = File::open(path).expect("open W9 source file");
    let mut block = [0_u8; 256 * 1024];
    loop {
        let read = file.read(&mut block).expect("read W9 source block");
        if read == 0 {
            break;
        }
        std::hint::black_box(block[0]);
    }
}

fn sequential_write(path: &str) {
    let mut file = File::create(path).expect("create W8 scratch file");
    let block = [0x5a_u8; 256 * 1024];
    for _ in 0..16 {
        file.write_all(&block).expect("write W8 scratch block");
    }
    file.sync_all().expect("sync W8 scratch file");
}
