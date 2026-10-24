export interface GridPosition {
  row: number;
  column: number;
}
export function gridTarget(
  current: GridPosition,
  key: string,
  shift: boolean,
  rows: number,
  columns: number,
): GridPosition | null {
  if (!rows || !columns) return null;
  let { row, column } = current;
  if (key === 'Tab') {
    const next = row * columns + column + (shift ? -1 : 1);
    return next < 0 || next >= rows * columns
      ? null
      : { row: Math.floor(next / columns), column: next % columns };
  }
  if (key === 'ArrowLeft') column--;
  else if (key === 'ArrowRight') column++;
  else if (key === 'ArrowUp') row--;
  else if (key === 'ArrowDown') row++;
  else if (key === 'Home') column = 0;
  else if (key === 'End') column = columns - 1;
  else return null;
  return {
    row: Math.max(0, Math.min(rows - 1, row)),
    column: Math.max(0, Math.min(columns - 1, column)),
  };
}
