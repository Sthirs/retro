/** Stroke icon shapes: SVG path data, or `rect:x,y,w,h,r` / `circle:cx,cy,r`. */
export const ICONS = {
  plus: ['M12 5v14M5 12h14'],
  calendar: ['M16 2v4M8 2v4M3 10h18', 'rect:3,4,18,18,2'],
  tasks: ['rect:3,3,18,18,2', 'm9 12 2 2 4-4'],
  settings: ['M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6'],
  chevronDown: ['m6 9 6 6 6-6'],
  bell: ['M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9', 'M10.3 21a1.94 1.94 0 0 0 3.4 0'],
  check: ['M20 6 9 17l-5-5'],
  close: ['M18 6 6 18M6 6l12 12'],
  copy: ['rect:9,9,12,12,2', 'M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1'],
  refresh: ['M21 12a9 9 0 1 1-2.6-6.4L21 8', 'M21 3v5h-5'],
  mail: ['rect:3,5,18,14,2', 'm3 7 9 6 9-6'],
  trash: ['M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6'],
  logout: ['M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9'],
  users: [
    'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2',
    'circle:9,7,4',
    'M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75',
  ],
  userPlus: ['M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2', 'circle:9,7,4', 'M19 8v6M22 11h-6'],
  shield: ['M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z'],
  alert: ['circle:12,12,9', 'M12 8v4M12 16h.01'],
  clock: ['circle:12,12,9', 'M12 7v5l3 2'],
  arrowRight: ['M5 12h14M13 6l6 6-6 6'],
  sun: [
    'circle:12,12,4',
    'M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4',
  ],
  moon: ['M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z'],
  monitor: ['rect:2,3,20,14,2', 'M8 21h8M12 17v4'],
} as const;

export type IconName = keyof typeof ICONS;
