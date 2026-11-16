// Historical delimited input is separate from the editable chart/table model.
// These adapters never read selection state or regenerate an existing source.
import { chartInput, rowsToCsv } from './chartdata.js';
import { escapeHtml, fileName } from './util.js';

function plainCell(html) {
  const node = document.createElement('div');
  node.innerHTML = String(html).replace(/<br\s*\/?>/gi, '\n');
  return node.textContent || '';
}

export function tableInput(content) {
  return {
    text: rowsToCsv(content.rows.map((row) => row.map(plainCell))),
    header: content.headerRow, transpose: false, delimiter: ',', dialect: 'csv', roles: [],
  };
}

/** Current content has an explicit schema; it is never inferred as an import. */
export const currentDataInput = (content) => content.kind === 'table' ? tableInput(content) : chartInput(content);

/** Store the exact supplied text and resolved interpretation, not normalized cells. */
export function dataSource(text, { name = '', dialect = 'csv', transpose = false, roles = [], table }) {
  return { text, name: fileName(name), delimiter: table.delimiter, dialect, header: table.header, transpose, roles: [...roles], edited: false };
}

export function sameInput(initial, text, table, transpose, dialect, roles) {
  return !!initial && text === initial.text && table?.header === initial.header && table?.delimiter === initial.delimiter
    && transpose === initial.transpose && dialect === initial.dialect && JSON.stringify(roles) === JSON.stringify(initial.roles);
}

/** CSV is text, even when a cell resembles HTML. Table formatting stays separate. */
export const tableFromData = (table, old = {}) => ({
  ...old, kind: 'table', rows: table.grid.map((row, i) => row.map((cell, j) => {
    const previous = old.rows?.[i]?.[j];
    return previous !== undefined && plainCell(previous) === cell ? previous : escapeHtml(cell).replace(/\n/g, '<br>');
  })), headerRow: table.header,
});
