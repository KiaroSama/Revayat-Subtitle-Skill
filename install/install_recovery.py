"""Explicit old-tree restoration of a noncommitted owned installation transaction."""
from __future__ import annotations

import logging
import os
from pathlib import Path
import shutil

import install_state as state
from runtime import write_json


def classify(record):
    target, stage, backup, disposal = (Path(record[key]) for key in ("target", "stage", "backup", "disposal"))
    previous, new = record["previous_id"], record["stage_id"]
    old_target = previous is not None and state.matches(target, previous, record["old"])
    old_backup = previous is not None and state.matches(backup, previous, record["old"])
    new_target = new is not None and state.matches(target, new, record["new"])
    new_stage = new is not None and state.matches(stage, new, record["new"])
    if os.path.lexists(backup) and not old_backup:
        raise ValueError("Recovery backup changed; preserve all transaction state")
    if os.path.lexists(target) and not old_target and not new_target:
        raise ValueError("Recovery target changed; preserve all transaction state")
    if os.path.lexists(stage) and not new_stage:
        # A killed copy can leave an owned incomplete stage. Never delete it by name.
        if new is None or state.identity(stage) != new:
            raise ValueError("Recovery staging identity is unknown; preserve all transaction state")
        state.tree_manifest(stage)  # Reject linked/special/unbounded partial state.
    if previous is not None and old_target == old_backup:
        raise ValueError("Recovery needs exactly one complete previous installation")
    if os.path.lexists(disposal):
        state.safe_path(disposal)
        if (new is None or state.identity(disposal) != new or new_target
                or record["phase"] not in {"restore-intent", "disposed", "restored"}):
            raise ValueError("Recovery disposal ownership is ambiguous; preserve state")
        state.tree_manifest(disposal)  # An owned partially cleaned quarantine is not a partial live target.
    if new_target and os.path.lexists(stage):
        raise ValueError("Recovery has ambiguous duplicate staged/publication state")
    return old_target, old_backup, new_target


def restore(journal: Path, root: Path, rename, *, explicit=True):
    state.registered(root, journal)
    data = state.load_journal(journal)
    if data["state"] == "committed":
        if explicit:
            raise ValueError("Committed installation is not interrupted; --recover does not authorize uninstall")
        return False
    if data["state"] == "restored":
        for record in data["records"]:
            target = Path(record["target"])
            if record["previous_id"] is None:
                if os.path.lexists(target):
                    raise ValueError("Restored absent target changed")
            elif not state.matches(target, record["previous_id"], record["old"]):
                raise ValueError("Restored previous installation changed")
        state.unregister(root, journal)
        cleanup_disposals(data)
        return True
    # Validate every old/new position before removing even one owned published tree.
    positions = [classify(record) for record in data["records"]]
    data["state"] = "restoring"
    state.save_journal(root, journal, data)
    for record, (_, old_backup, new_target) in reversed(list(zip(data["records"], positions))):
        target, backup = Path(record["target"]), Path(record["backup"])
        record["phase"] = "restore-intent"
        state.save_journal(root, journal, data)
        classify(record)
        if new_target:
            if not state.matches(target, record["stage_id"], record["new"]):
                raise ValueError("Recovery publication changed before removal")
            disposal = Path(record["disposal"])
            if os.path.lexists(disposal):
                raise ValueError("Recovery disposal path became occupied")
            state.move(target, disposal, rename)
            record["phase"] = "disposed"
            state.save_journal(root, journal, data)
        if old_backup:
            if os.path.lexists(target) or not state.matches(backup, record["previous_id"], record["old"]):
                raise ValueError("Recovery old-tree destination changed")
            state.move(backup, target, rename)
        if record["previous_id"] is None:
            if os.path.lexists(target):
                raise ValueError("Recovery did not restore original absence")
        elif not state.matches(target, record["previous_id"], record["old"]):
            raise ValueError("Recovery did not restore complete previous bytes")
        record["phase"] = "restored"
        state.save_journal(root, journal, data)
    data["state"] = "restored"
    state.save_journal(root, journal, data)
    state.unregister(root, journal)
    cleanup_disposals(data)
    logging.info("Installation restored targets=%d journal=%s", len(data["records"]), journal)
    return True


def cleanup_disposals(data):
    for record in data["records"]:
        disposal = Path(record["disposal"])
        try:
            state.safe_path(disposal)
            if disposal.exists() and state.identity(disposal) == record["stage_id"]:
                state.tree_manifest(disposal)
                shutil.rmtree(disposal)
        except (OSError, ValueError):
            logging.warning("Recovered old installation; owned disposal cleanup pending: %s", disposal)


def recover(journal: Path, recovery: Path, rename):
    with state.ownership() as root:
        journal = state.safe_path(journal, directory=False)
        data = state.load_journal(journal)
        if state.safe_path(recovery) != Path(data["recovery"]):
            raise ValueError("--recovery-dir differs from the journal root")
        return restore(journal, root, rename)
