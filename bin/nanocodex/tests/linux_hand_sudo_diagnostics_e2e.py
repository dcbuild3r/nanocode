#!/usr/bin/env python3
"""Linux Hand setup must explain a sudo password it cannot ask for.

Runs the real CLI without a controlling terminal (setsid, stdin=/dev/null),
with a PATH-first fake sudo that behaves like password-protected sudo. No
service, account, network, or sudo policy is touched. Usage:
  python3 bin/nanocodex/tests/linux_hand_sudo_diagnostics_e2e.py CLI [OUTPUT_DIR]
"""
import os, pathlib, subprocess, sys, tempfile, time

cli = pathlib.Path(sys.argv[1]).resolve()
output = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else "output/linux-hand-sudo-diagnostics")
output.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="nanocodex-sudo-journey-") as fixture:
    fixture = pathlib.Path(fixture)
    bin_dir = fixture / "bin"; bin_dir.mkdir()
    calls = fixture / "sudo-calls.log"
    sudo = bin_dir / "sudo"
    sudo.write_text(
        "#!/bin/sh\n"
        f"echo \"$*\" >> {calls}\n"
        "echo 'sudo: a terminal is required to read the password; either use ssh'\"'\"'s -t option or configure an askpass helper' >&2\n"
        "echo 'sudo: a password is required' >&2\n"
        "exit 1\n")
    sudo.chmod(0o755)
    store = fixture / "install"; store.mkdir()
    (store / "automatic-updates-disabled").write_text("")
    env = {"PATH": f"{bin_dir}:/usr/bin:/bin", "HOME": str(fixture), "NANOCODEX_DIR": str(store),
           "NO_COLOR": "1", "TMPDIR": str(fixture)}
    start = time.monotonic()
    result = subprocess.run([str(cli), "hand", "install", "--prepare"], env=env, cwd=fixture,
                            stdin=subprocess.DEVNULL, capture_output=True, text=True,
                            timeout=30, start_new_session=True)
    elapsed = round((time.monotonic() - start) * 1000)
    sudo_calls = calls.read_text().splitlines() if calls.exists() else []
    transcript = (f"$ nanocodex hand install --prepare  (no controlling terminal)\nexit: {result.returncode}; elapsed: {elapsed} ms\n"
                  f"sudo calls: {sudo_calls}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}\n")
    (output / "transcript.log").write_text(transcript)
    print(transcript)
    err = result.stderr
    assert result.returncode != 0, "setup must fail without administrator access"
    assert "stage administrator access" in err, "names the failed stage"
    assert "no terminal to ask for it" in err, "names the concrete cause"
    assert "State: the Hand service was not installed or changed." in err, "reports resulting state"
    assert "Retryable: yes" in err, "reports retryability"
    assert "ssh -t HOST nanocodex setup" in err and "--target root@HOST" in err, "gives exact next commands"
    assert "sudoers" not in err.lower() and "NOPASSWD" not in err, "never suggests weakening sudo"
    assert sudo_calls == ["-n true"], f"must not prompt without a terminal: {sudo_calls}"
    assert elapsed < 10_000
print("PASS linux hand sudo diagnostics journey")

