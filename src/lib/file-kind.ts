import {
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileImage,
  FileSpreadsheet,
  FileText,
  FileVideo,
} from "lucide-solid";

/**
 * Which glyph a transfer gets, chosen from its extension.
 *
 * A single document icon for every file told the user nothing: a 2 GB video
 * and a text note looked identical in the conversation. The categories match
 * what people actually sort by, and `tone` gives the icon a colour without
 * making it decorative noise.
 */
const VIDEO = new Set([
  "mp4",
  "mkv",
  "mov",
  "avi",
  "webm",
  "m4v",
  "wmv",
  "flv",
  "mpg",
  "mpeg",
  "ts",
]);
const AUDIO = new Set(["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma", "aiff"]);
const IMAGE = new Set(["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "heic", "avif", "tiff"]);
const ARCHIVE = new Set(["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst"]);
const SHEET = new Set(["xls", "xlsx", "csv", "ods"]);
const CODE = new Set([
  "rs",
  "ts",
  "tsx",
  "js",
  "jsx",
  "py",
  "go",
  "java",
  "kt",
  "c",
  "h",
  "cpp",
  "hpp",
  "cs",
  "rb",
  "php",
  "swift",
  "sh",
  "ps1",
  "json",
  "toml",
  "yaml",
  "yml",
  "xml",
  "html",
  "css",
]);
const TEXT = new Set(["txt", "md", "log", "pdf", "doc", "docx", "rtf", "odt", "epub"]);

export type FileTone = "video" | "audio" | "image" | "archive" | "sheet" | "code" | "text" | "file";

export function fileKind(name: string): { icon: typeof File; tone: FileTone } {
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  if (VIDEO.has(ext)) return { icon: FileVideo, tone: "video" };
  if (AUDIO.has(ext)) return { icon: FileAudio, tone: "audio" };
  if (IMAGE.has(ext)) return { icon: FileImage, tone: "image" };
  if (ARCHIVE.has(ext)) return { icon: FileArchive, tone: "archive" };
  if (SHEET.has(ext)) return { icon: FileSpreadsheet, tone: "sheet" };
  if (CODE.has(ext)) return { icon: FileCode, tone: "code" };
  if (TEXT.has(ext)) return { icon: FileText, tone: "text" };
  return { icon: File, tone: "file" };
}
