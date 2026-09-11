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

## Why the encode is ours

vhs captures the frames and this repo's `record.sh` runs ffmpeg, rather than
letting vhs do both.

vhs 0.12 will not encode against ffmpeg 9. It captures every frame correctly,
prints `Creating x.gif...`, exits 0, and writes nothing — it never invokes
ffmpeg at all, and never says why. Diagnosing that cost an hour: ffmpeg was
fine, the frames were fine, and the failure was invisible from both ends.

So each tape's `Output` is rewritten to a frames directory, and the encode
composites the text and cursor layers over a background, pads, and builds a
palette. The text layer is glyphs on transparency, which is the one thing to
know if you touch that filter chain: composite it straight onto the backdrop
and the backdrop colour shows between every letter.

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
