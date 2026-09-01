/**
 * Every widget there is.
 *
 * One list, read by the board and by the chin, so a widget added here turns up
 * in both without anybody remembering. That is deliberate and it is the fourth
 * time this codebase has learned it: a hand-written list of the modes that
 * draw a result list, an icon allowlist, a subtitle allowlist and a kind list
 * all silently did nothing for a thing somebody had just added.
 *
 * The components are not in here. A registry that imports every widget makes
 * the chin pull in the board's dependencies and vice versa; the two surfaces
 * pick their own components by id and this holds only what both need to know.
 */
export type WidgetId = "clock" | "weather" | "machine";

export interface WidgetInfo {
  id: WidgetId;
  /** What it is called, in the board's corner and in settings. */
  name: string;
  /** One line, for the settings list where it is chosen. */
  blurb: string;
  /** Takes both columns of the board, for a widget with a list in it. */
  wide?: boolean;
}

export const WIDGETS: WidgetInfo[] = [
  {
    id: "clock",
    name: "Clock",
    blurb: "The time, the date, and how far through the day it is",
  },
  {
    id: "weather",
    name: "Weather",
    blurb: "Now, and today's high and low, for a place you choose",
  },
  {
    id: "machine",
    name: "This machine",
    blurb: "Processor, memory, the heaviest programs, and what Sill costs",
    wide: true,
  },
];

/** One widget by id, or nothing when the id is from a newer build. */
export function widget(id: string): WidgetInfo | undefined {
  return WIDGETS.find((one) => one.id === id);
}
