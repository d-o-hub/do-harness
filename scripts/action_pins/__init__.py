# action_pins package — the immutable-action-reference checker.
#
# `scripts/check-action-pins.py` is the executable entry point; the parts
# live here so each file stays reviewable:
#
#   `findings`   the Finding record and the single-line diagnostic escaping
#   `discovery`  which workflow files are read at all
#   `references` the policy: what a reference may say
#   `structure`  the workflow shapes: where a reference may hide
#   `checker`    the per-file walk and the run's exit decision
