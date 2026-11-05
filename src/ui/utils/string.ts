/**
 *
 *
 */
export function toKebabCase(key: string): string {
  if (key.startsWith("--")) return key;
  if (key === "float" || key === "cssFloat") return "float";

  const normalized = /^[A-Z]/.test(key) ? `-${key[0].toLowerCase()}${key.slice(1)}` : key;
  const kebab = normalized.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`);
  return kebab.startsWith("ms-") ? `-${kebab}` : kebab;
}
