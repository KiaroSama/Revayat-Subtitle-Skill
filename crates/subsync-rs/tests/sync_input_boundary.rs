mod support;

#[test]
fn ordinary_regular_inputs_still_align() {
    let fixture = support::Fixture::new();
    fixture.write_subtitle("\n");
    assert_eq!(
        support::report(support::run(fixture.command()))["cues_total"],
        4
    );
}

#[test]
fn oversized_subtitle_and_wav_are_refused_before_reading() {
    for subtitle in [true, false] {
        let fixture = support::Fixture::new();
        fixture.write_subtitle("\n");
        let (path, limit) = if subtitle {
            (&fixture.subtitle, subsync_rs::MAX_SUBTITLE_BYTES)
        } else {
            (&fixture.wav, subsync_rs::audio::MAX_WAV_BYTES)
        };
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_len(limit + 1)
            .unwrap();
        let result = support::run(fixture.command());
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("size limit"));
        assert!(!fixture.output.exists());
        assert!(!fixture.receipt.exists());
    }
}

#[cfg(unix)]
#[test]
fn user_selected_symlinks_to_regular_inputs_remain_supported() {
    let fixture = support::Fixture::new();
    fixture.write_subtitle("\n");
    for path in [&fixture.subtitle, &fixture.wav] {
        let target = path.with_extension("actual");
        std::fs::rename(path, &target).unwrap();
        std::os::unix::fs::symlink(target, path).unwrap();
    }
    assert_eq!(
        support::report(support::run(fixture.command()))["cues_total"],
        4
    );
}

#[cfg(target_os = "linux")]
fn refuse_fifo(subtitle: bool) {
    use std::{
        fs,
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    let mut fixture = support::Fixture::new();
    fixture.write_subtitle("\n");
    let fifo = fixture.directory.path().join(if subtitle {
        "blocked.srt"
    } else {
        "blocked.wav"
    });
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    if subtitle {
        fixture.subtitle = fifo;
    } else {
        fixture.wav = fifo;
    }
    let stderr = fixture.directory.path().join("stderr");
    let mut child = fixture
        .command()
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let status = status.expect("A non-regular input must be refused without opening its FIFO");
    assert!(!status.success());
    let message = fs::read_to_string(stderr).unwrap();
    assert!(message.contains("regular file"), "{message}");
    assert!(!fixture.output.exists());
    assert!(!fixture.receipt.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn subtitle_fifo_is_refused_without_blocking() {
    refuse_fifo(true);
}

#[cfg(target_os = "linux")]
#[test]
fn wav_fifo_is_refused_without_blocking() {
    refuse_fifo(false);
}
