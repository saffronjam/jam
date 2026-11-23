"""A real foreground terminal program emitting public OSC status records."""

import json
import shlex
import sys
from pathlib import Path


def main():
    provider, output, *mode = sys.argv[1:]
    output = Path(output)

    def emit(state):
        sys.stdout.write(f"\x1b]7501;state={state}:app={provider}\x07")
        sys.stdout.write(f"\r\nPROGRAM {state.upper()}\r\n")
        sys.stdout.flush()
        temporary = output.with_suffix(".tmp")
        temporary.write_text(json.dumps({"phase": state}))
        temporary.replace(output)

    emit("working")
    for line in sys.stdin:
        command = line.strip()
        if command == "exit":
            break
        if mode == ["image-arguments"]:
            args = shlex.split(command)
            if len(args) != 2:
                raise ValueError("Expected the image path and one following argument")
            Path("image-path.txt").write_text("\n".join(args) + "\n")
            break
        if command not in {"done", "clear", "idle", "working"}:
            raise ValueError(command)
        emit(command)


if __name__ == "__main__":
    main()
