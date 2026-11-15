/** "Marco" from "Marco Bianchi", for short labels such as "Skip Marco". */
export function firstName(displayName: string): string {
  return displayName.trim().split(/\s+/)[0] ?? displayName;
}
