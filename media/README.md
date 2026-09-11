# Recording the GIFs

Generated rather than hand-captured, so they can be redone after a UI change
without anyone having to remember what was in the last one.

```sh
brew install vhs          # pulls ttyd and ffmpeg
sh media/record.sh        # builds, records all three, checks the output
```

Then uncomment the `<img>` tags in `README.md`.

## It has to be a terminal you are sitting in front of

vhs drives a headless Chrome, and Chrome needs a macOS window server session to
render. Run this over SSH, from a daemon, or from anything else without one and
**vhs reports success, captures zero frames, and writes nothing**. `record.sh`
checks the file is non-empty and says so, because vhs itself will not.

## What gets recorded

`fixture.sh` builds the workspace first: a fifteen-note vault with a real
shape (projects carrying their own decisions and research, a knowledge
section, dailies, an archive) and a git repository with uncommitted work in
it, so the diff column has something to show. Without it the tapes recorded an
empty app, which sells nothing.

Everything lands in a `mktemp -d`, including `HOUSTON_STATE_DIR`. **Do not
remove that.** Without it a recording writes demo sessions into your real
session list.

## The agents are real

`sessions.tape` starts actual Claude Code sessions and asks them short
questions. That means:

- **It costs a few tokens.** Not many; the questions are one-liners.
- **The timings are guesses.** Every `Sleep` after a prompt is slack for a
  response that takes as long as it takes. If a recording catches an agent
  mid-sentence, make the `Sleep` longer rather than trimming the tape. A GIF
  that cuts an agent off looks broken.
- **Re-recording gives different answers.** That is fine, but watch the output
  before committing it: whatever the agent says goes in the README.

`vault.tape` and `editor.tape` use no agents and are deterministic.

## The frame

All three share the block in `_style.md`: a rounded window with a coloured
title bar, sitting on a dark margin. VHS has no include directive, so it is
pasted into each tape. Change one, change all three.
