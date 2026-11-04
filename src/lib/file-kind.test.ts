import { describe, expect, it } from "vitest";
import { fileKind } from "./file-kind";

describe("fileKind", () => {
  it("maps the extensions people actually transfer", () => {
    expect(fileKind("movie.mkv").tone).toBe("video");
    expect(fileKind("track.flac").tone).toBe("audio");
    expect(fileKind("shot.png").tone).toBe("image");
    expect(fileKind("backup.7z").tone).toBe("archive");
    expect(fileKind("budget.xlsx").tone).toBe("sheet");
    expect(fileKind("main.rs").tone).toBe("code");
    expect(fileKind("notes.md").tone).toBe("text");
  });

  it("is case-insensitive and ignores the path", () => {
    expect(fileKind("Holiday.MP4").tone).toBe("video");
    expect(fileKind("a.b.PNG").tone).toBe("image");
  });

  it("falls back to a plain file when the extension means nothing", () => {
    expect(fileKind("README").tone).toBe("file");
    expect(fileKind("data.xyzzy").tone).toBe("file");
    expect(fileKind("trailingdot.").tone).toBe("file");
  });
});
