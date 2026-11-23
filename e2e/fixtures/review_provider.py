"""Hosting CLI protocol simulator; it performs no network requests."""

import json
import os
import sys
from pathlib import Path


def main():
    tool = Path(sys.argv[0]).name
    host = "github.com" if tool == "gh" else "gitlab.com"
    if sys.argv[1:6] != ["api", "--hostname", host, "--method", "GET"] or len(sys.argv) != 7:
        raise SystemExit("Unexpected hosting CLI request")
    route = sys.argv[-1]
    with (Path(os.environ["HOME"]) / "review-events.jsonl").open("a") as log:
        log.write(json.dumps({"tool": tool, "route": route}) + "\n")
    if tool == "gh":
        if route == "repos/fixture/project":
            value = {"full_name": "fixture/project"}
        elif route.startswith("repos/fixture/project/pulls?"):
            value = [
                {
                    "number": 42,
                    "state": "open",
                    "head": {"ref": "main", "repo": {"full_name": "fixture/project"}},
                    "html_url": "https://github.com/fixture/project/pull/42",
                    "title": "Fixture review",
                    "draft": False,
                }
            ]
        else:
            raise SystemExit("Unexpected GitHub route: " + route)
    elif route == "projects/fixture%2Fproject":
        value = {"id": 7}
    elif route.startswith("projects/7/merge_requests?"):
        value = [
            {
                "iid": 42,
                "state": "opened",
                "source_branch": "main",
                "source_project_id": 7,
                "web_url": "https://gitlab.com/fixture/project/-/merge_requests/42",
                "title": "Fixture review",
                "draft": False,
            }
        ]
    else:
        raise SystemExit("Unexpected GitLab route: " + route)
    print(json.dumps(value))


if __name__ == "__main__":
    main()
