import { toKebabCase } from "./string";

/**
 *
 * @example
 * const el = document.getElementById('myDiv');
 * getStyleValue(el, 'color');
 * getStyleValue(el, 'backgroundColor');
 */
export function getStyleValue(element: Element | null, property: string): string | null {
  if (typeof window === "undefined" || !window.getComputedStyle) {
    console.warn("getStyleValue: getComputedStyle is unavailable in this environment");
    return null;
  }

  if (!element) {
    console.warn("getStyleValue: the element is not valid (null or undefined)");
    return null;
  }

  if (typeof property !== "string" || property.trim() === "") {
    console.warn("getStyleValue: the property name must be a non-empty string");
    return null;
  }

  try {
    const computedStyle = window.getComputedStyle(element);
    const value = computedStyle.getPropertyValue(toKebabCase(property.trim()));
    return value || null;
  } catch (error) {
    console.error(`getStyleValue: reading the style threw - ${error}`);
    return null;
  }
}
