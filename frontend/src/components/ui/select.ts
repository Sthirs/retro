/** One choice of a `SelectMenu`. */
export interface SelectOption {
  value: string;
  label: string;
  /** A second, muted line under the label. */
  hint?: string;
}
