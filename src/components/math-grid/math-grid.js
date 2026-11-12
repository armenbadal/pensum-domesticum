const DEFAULT_ROWS = 10;
const DEFAULT_COLUMNS = 20;
const LEGACY_ROWS = 20;
const ALLOWED_CHARACTER = /^[0-9+\-−*/×÷=()]$/u;

class MathGrid extends HTMLElement {
  static formAssociated = true;

  static observedAttributes = ["required", "disabled"];

  #internals;
  #cells = [];
  #rows = DEFAULT_ROWS;
  #columns = DEFAULT_COLUMNS;
  #value = "";
  #grid;
  #message;
  #formDisabled = false;
  #addRowButton;
  #removeRowButton;
  #addColumnButton;
  #removeColumnButton;
  #thickenBottomButton;
  #thickenLeftButton;
  #clearBordersButton;
  #selectedCells = new Set();
  #thickBorders = new Set();
  #selectionAnchor = 0;
  #activeCell = 0;
  #isDraggingSelection = false;
  #stopCellSelection = () => {
    this.#isDraggingSelection = false;
  };

  constructor() {
    super();
    this.#internals = this.attachInternals();

    const root = this.attachShadow({ mode: "open" });
    root.innerHTML = `
      <style>
        :host {
          display: block;
          --cell-size: 6mm;
        }

        .grid {
          display: grid;
          grid-template-columns: repeat(${DEFAULT_COLUMNS}, var(--cell-size));
          grid-template-rows: repeat(${DEFAULT_ROWS}, var(--cell-size));
          width: max-content;
          border-top: 1px solid #c5cbd3;
          border-left: 1px solid #c5cbd3;
          background: white;
        }

        :host([disabled]) .grid-controls {
          display: none;
        }

        .grid-controls {
          display: flex;
          max-width: 100%;
          align-items: center;
          gap: 6px;
          margin: 6px 0 0;
          overflow-x: auto;
        }

        .dimension-controls {
          display: flex;
          flex: 0 0 auto;
          flex-wrap: nowrap;
          gap: 6px;
        }

        .dimension-controls button,
        .border-controls button {
          box-sizing: border-box;
          display: inline-grid;
          width: 28px;
          min-height: 28px;
          height: 28px;
          padding: 2px;
          border: 1px solid #c5cbd3;
          border-radius: 6px;
          color: #3159c9;
          background: white;
          place-items: center;
        }

        .dimension-controls button:not(:disabled),
        .border-controls button:not(:disabled) {
          cursor: pointer;
        }

        .dimension-controls button:hover:not(:disabled),
        .border-controls button:hover:not(:disabled) {
          border-color: #3159c9;
          background: #f1f5ff;
        }

        .dimension-controls button:disabled,
        .border-controls button:disabled {
          cursor: not-allowed;
        }

        .dimension-controls img {
          width: 22px;
          height: 22px;
        }

        .border-controls svg {
          width: 22px;
          height: 22px;
          fill: none;
          stroke: currentColor;
          stroke-linecap: square;
          stroke-linejoin: miter;
          stroke-width: 1.5;
        }

        .dimension-controls button:focus-visible,
        .border-controls button:focus-visible {
          outline: 3px solid rgb(49 89 201 / 22%);
          outline-offset: 1px;
        }

        .grid input {
          box-sizing: border-box;
          width: var(--cell-size);
          height: var(--cell-size);
          min-width: 0;
          padding: 0;
          border: 0;
          border-right: 1px solid #c5cbd3;
          border-bottom: 1px solid #c5cbd3;
          border-radius: 0;
          background: transparent;
          color: #111;
          font: 12px/1 system-ui, sans-serif;
          text-align: center;
          text-transform: none;
        }

        .grid input:focus {
          position: relative;
          z-index: 1;
          outline: 2px solid #1769aa;
          outline-offset: -2px;
        }

        .grid input:disabled {
          background: #eee;
        }

        .grid input.selected-cell {
          background: #dceeff;
          outline: 1px solid #1769aa;
          outline-offset: -2px;
          position: relative;
          z-index: 1;
        }

        .grid input.active-cell {
          outline: 2px solid #1769aa;
          outline-offset: -2px;
          z-index: 2;
        }

        .grid input.thick-left {
          border-left: 2px solid #111;
        }

        .grid input.thick-bottom {
          border-bottom: 2px solid #111;
        }

        .border-controls {
          display: flex;
          flex: 0 0 auto;
          flex-wrap: nowrap;
          gap: 6px;
        }

        .message {
          min-height: 1.5em;
          margin: 6px 0 0;
          color: #b3261e;
          font: 0.875rem/1.5 system-ui, sans-serif;
        }

        .message:empty {
          display: none;
        }
      </style>
      <div class="grid" role="group"></div>
      <div class="grid-controls">
        <div class="dimension-controls">
          <button class="add-row" type="button" aria-label="Ավելացնել տող" title="Ավելացնել տող"></button>
          <button class="remove-row" type="button" aria-label="Հեռացնել վերջին տողը" title="Հեռացնել վերջին տողը"></button>
          <button class="add-column" type="button" aria-label="Ավելացնել սյուն" title="Ավելացնել սյուն"></button>
          <button class="remove-column" type="button" aria-label="Հեռացնել վերջին սյունը" title="Հեռացնել վերջին սյունը"></button>
        </div>
        <div class="border-controls">
          <button class="thicken-bottom" type="button" aria-label="Հաստացնել ներքևի եզրագիծը" title="Հաստացնել ներքևի եզրագիծը" disabled>
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <rect x="3.75" y="3.75" width="16.5" height="16.5"></rect>
              <path d="M4 17h16" stroke-width="3"></path>
            </svg>
          </button>
          <button class="thicken-left" type="button" aria-label="Հաստացնել ձախ եզրագիծը" title="Հաստացնել ձախ եզրագիծը" disabled>
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <rect x="3.75" y="3.75" width="16.5" height="16.5"></rect>
              <path d="M7 4v16" stroke-width="3"></path>
            </svg>
          </button>
          <button class="clear-borders" type="button" aria-label="Մաքրել ընտրված տիրույթի եզրագծերը" title="Մաքրել ընտրված տիրույթի եզրագծերը" disabled>
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="M4 5h16M4 11h16M4 17h10M7 3v16M13 3v9"></path>
              <path d="m15.5 15.5 5 5m0-5-5 5" stroke-width="2"></path>
            </svg>
          </button>
        </div>
      </div>
      <p class="message" aria-live="polite"></p>
    `;

    this.#grid = root.querySelector(".grid");
    this.#message = root.querySelector(".message");
    this.#addRowButton = root.querySelector(".add-row");
    this.#removeRowButton = root.querySelector(".remove-row");
    this.#addColumnButton = root.querySelector(".add-column");
    this.#removeColumnButton = root.querySelector(".remove-column");
    this.#thickenBottomButton = root.querySelector(".thicken-bottom");
    this.#thickenLeftButton = root.querySelector(".thicken-left");
    this.#clearBordersButton = root.querySelector(".clear-borders");
    this.#addRowButton.append(this.#createDimensionIcon("rows-3-plus"));
    this.#removeRowButton.append(this.#createDimensionIcon("rows-3-minus"));
    this.#addColumnButton.append(this.#createDimensionIcon("columns-3-plus"));
    this.#removeColumnButton.append(this.#createDimensionIcon("columns-3-minus"));
    this.#addRowButton.addEventListener("click", () => this.#resizeGrid(this.#rows + 1, this.#columns));
    this.#removeRowButton.addEventListener("click", () => {
      if (this.#rows > 1) {
        this.#resizeGrid(this.#rows - 1, this.#columns);
      }
    });
    this.#addColumnButton.addEventListener("click", () => this.#resizeGrid(this.#rows, this.#columns + 1));
    this.#removeColumnButton.addEventListener("click", () => {
      if (this.#columns > 1) {
        this.#resizeGrid(this.#rows, this.#columns - 1);
      }
    });
    this.#thickenBottomButton.addEventListener("click", () => this.#applySelectedBorder("bottom"));
    this.#thickenLeftButton.addEventListener("click", () => this.#applySelectedBorder("left"));
    this.#clearBordersButton.addEventListener("click", () => this.#clearSelectedBorders());

    this.#renderGrid();
    this.#updateCellStyles();
    this.#updateFormValue();
  }

  get value() {
    return this.#value;
  }

  set value(value) {
    const state = typeof value === "string" ? value : JSON.stringify(value);
    this.#restoreFormState(state);
  }

  #createDimensionIcon(iconName) {
    const icon = document.createElement("img");
    icon.src = new URL(`../../assets/icons/${iconName}.svg`, import.meta.url).href;
    icon.alt = "";
    icon.draggable = false;
    return icon;
  }

  #createCell(index) {
    const input = document.createElement("input");
    const row = Math.floor(index / this.#columns) + 1;
    const column = (index % this.#columns) + 1;
    input.type = "text";
    input.maxLength = 1;
    input.autocomplete = "off";
    input.spellcheck = false;
    input.inputMode = "text";
    input.tabIndex = index === 0 ? 0 : -1;
    input.setAttribute("aria-label", `Տող ${row}, սյուն ${column}`);
    input.addEventListener("beforeinput", (event) => this.#handleBeforeInput(event));
    input.addEventListener("input", (event) => {
      event.stopPropagation();
      this.#handleInput(input, index);
    });
    input.addEventListener("keydown", (event) => this.#handleKeydown(event, index));
    input.addEventListener("paste", (event) => this.#handlePaste(event, index));
    input.addEventListener("pointerdown", (event) => this.#beginCellSelection(event, index));
    input.addEventListener("pointerenter", () => this.#extendCellSelection(index));
    input.addEventListener("focus", () => {
      if (index !== this.#activeCell && !this.#isDraggingSelection) {
        this.#selectionAnchor = index;
        this.#activeCell = index;
        this.#selectRange(index, index);
      }
      this.#focusCell(index);
      input.select();
    });
    return input;
  }

  #renderGrid(values = []) {
    this.#cells = [];
    this.#grid.style.gridTemplateColumns = `repeat(${this.#columns}, var(--cell-size))`;
    this.#grid.style.gridTemplateRows = `repeat(${this.#rows}, var(--cell-size))`;
    this.#grid.setAttribute(
      "aria-label",
      this.getAttribute("aria-label") ?? `${this.#rows} տող, ${this.#columns} սյուն`,
    );
    const fragment = document.createDocumentFragment();
    for (let index = 0; index < this.#rows * this.#columns; index += 1) {
      const input = this.#createCell(index);
      input.value = values[index] ?? "";
      fragment.append(input);
      this.#cells.push(input);
    }
    this.#grid.replaceChildren(fragment);
  }

  #resizeGrid(rows, columns) {
    const values = Array(rows * columns).fill("");
    for (let row = 0; row < Math.min(rows, this.#rows); row += 1) {
      for (let column = 0; column < Math.min(columns, this.#columns); column += 1) {
        values[row * columns + column] = this.#cells[row * this.#columns + column].value;
      }
    }

    const borders = new Set();
    for (const border of this.#thickBorders) {
      const [oldIndex, edge] = border.split(":");
      const row = Math.floor(Number(oldIndex) / this.#columns);
      const column = Number(oldIndex) % this.#columns;
      if (row < rows && column < columns) {
        borders.add(`${row * columns + column}:${edge}`);
      }
    }

    this.#rows = rows;
    this.#columns = columns;
    this.#thickBorders = borders;
    this.#selectedCells.clear();
    this.#selectedCells.add(0);
    this.#selectionAnchor = 0;
    this.#activeCell = 0;
    this.#renderGrid(values);
    this.#updateCellStyles();
    this.#updateDisabledState();
    this.#commitValue();
  }

  connectedCallback() {
    document.addEventListener("pointerup", this.#stopCellSelection);
    document.addEventListener("pointercancel", this.#stopCellSelection);
    this.#updateDisabledState();
    this.#updateValidity();
  }

  disconnectedCallback() {
    document.removeEventListener("pointerup", this.#stopCellSelection);
    document.removeEventListener("pointercancel", this.#stopCellSelection);
  }

  attributeChangedCallback() {
    this.#updateDisabledState();
    this.#updateValidity();
  }

  formResetCallback() {
    for (const cell of this.#cells) {
      cell.value = "";
    }
    this.#thickBorders.clear();
    this.#selectionAnchor = 0;
    this.#activeCell = 0;
    this.#selectedCells.clear();
    this.#selectedCells.add(0);
    this.#clearMessage();
    this.#updateCellStyles();
    this.#updateFormValue();
    this.#updateValidity();
  }

  formDisabledCallback(disabled) {
    this.#formDisabled = disabled;
    this.#updateDisabledState();
  }

  formStateRestoreCallback(state) {
    if (typeof state === "string") {
      this.#restoreFormState(state);
    }
  }

  #restoreFormState(state) {
    let restored;
    try {
      restored = JSON.parse(state);
    } catch (error) {
      console.error("Չհաջողվեց վերականգնել math-grid-ի form տվյալները։", error);
      return;
    }

    const rowCount = restored?.version === 1 ? LEGACY_ROWS : restored?.rows;
    const columnCount = restored?.version === 1 ? DEFAULT_COLUMNS : restored?.columns;
    if (
      !restored
      || ![1, 2].includes(restored.version)
      || !Number.isSafeInteger(rowCount)
      || rowCount < 1
      || !Number.isSafeInteger(columnCount)
      || columnCount < 1
      || typeof restored.cells !== "string"
      || !Array.isArray(restored.borders)
    ) {
      console.error("math-grid-ի form տվյալների ձևաչափը վավեր չէ։");
      return;
    }

    const rows = restored.cells.split("\n");
    const validRows = rows.length === rowCount
      && rows.every((row) => [...row].length === columnCount
        && [...row].every((character) => character === " " || ALLOWED_CHARACTER.test(character)));
    const validBorders = restored.borders.every((border) => border
      && Number.isInteger(border.cell)
      && border.cell >= 0
      && border.cell < rowCount * columnCount
      && ["bottom", "left"].includes(border.edge));
    if (!validRows || !validBorders) {
      console.error("math-grid-ի form տվյալների վանդակները կամ եզրագծերը վավեր չեն։");
      return;
    }

    this.#rows = rowCount;
    this.#columns = columnCount;
    this.#thickBorders = new Set(
      restored.borders.map(({ cell, edge }) => `${cell}:${edge}`),
    );
    this.#selectedCells.clear();
    this.#activeCell = 0;
    this.#selectionAnchor = 0;
    this.#selectedCells.add(0);
    this.#renderGrid(rows.flatMap((row) => [...row].map((character) => character === " " ? "" : character)));
    this.#updateCellStyles();
    this.#updateFormValue();
    this.#updateDisabledState();
    this.#updateValidity();
  }

  #handleBeforeInput(event) {
    if (event.inputType.startsWith("insert") && event.data && [...event.data].some((character) => !ALLOWED_CHARACTER.test(character))) {
      event.preventDefault();
      this.#showMessage("Թույլատրվում են միայն թվանշաններն ու մաթեմատիկական նշանները։");
    }
  }

  #handleInput(cell, index) {
    if (cell.value && !ALLOWED_CHARACTER.test(cell.value)) {
      cell.value = "";
      this.#showMessage("Թույլատրվում են միայն թվանշաններն ու մաթեմատիկական նշանները։");
    } else {
      this.#clearMessage();
    }

    this.#commitValue();

    if (cell.value && index < this.#cells.length - 1) {
      const nextIndex = index + 1;
      this.#selectionAnchor = nextIndex;
      this.#activeCell = nextIndex;
      this.#selectRange(nextIndex, nextIndex);
      this.#focusCell(nextIndex);
    }
  }

  #handleKeydown(event, index) {
    const targetByKey = {
      ArrowLeft: index - 1,
      ArrowRight: index + 1,
      ArrowUp: index - this.#columns,
      ArrowDown: index + this.#columns,
    };

    if (Object.hasOwn(targetByKey, event.key)) {
      const target = targetByKey[event.key];
      if (target >= 0 && target < this.#cells.length) {
        event.preventDefault();
        if (!event.shiftKey) {
          this.#selectionAnchor = target;
        }
        this.#setActiveCell(target);
        this.#selectRange(this.#selectionAnchor, target);
        this.#focusCell(target);
      }
      return;
    }

    if (event.key === "Backspace" && !event.currentTarget.value && index > 0) {
      event.preventDefault();
      this.#cells[index - 1].value = "";
      this.#commitValue();
      this.#focusCell(index - 1);
    }
  }

  #handlePaste(event, index) {
    const text = event.clipboardData?.getData("text");
    if (text === undefined) {
      return;
    }

    event.preventDefault();
    const characters = [...text].filter((character) => !/\s/u.test(character));
    if (characters.some((character) => !ALLOWED_CHARACTER.test(character))) {
      this.#showMessage("Տեղադրվող տեքստը պարունակում է չթույլատրված նիշ։");
      return;
    }
    if (index + characters.length > this.#cells.length) {
      this.#showMessage("Տեքստը չի տեղավորվում մնացած վանդակներում։");
      return;
    }

    characters.forEach((character, offset) => {
      this.#cells[index + offset].value = character;
    });
    this.#clearMessage();
    this.#commitValue();
    this.#focusCell(Math.min(index + characters.length, this.#cells.length - 1));
  }

  #focusCell(index) {
    this.#cells.forEach((cell, cellIndex) => {
      cell.tabIndex = cellIndex === index ? 0 : -1;
    });
    this.#cells[index].focus();
  }

  #updateFormValue() {
    const rows = [];
    for (let row = 0; row < this.#rows; row += 1) {
      const start = row * this.#columns;
      rows.push(this.#cells.slice(start, start + this.#columns).map((cell) => cell.value || " ").join(""));
    }
    const borders = [...this.#thickBorders]
      .map((border) => {
        const [cell, edge] = border.split(":");
        return { cell: Number(cell), edge };
      })
      .sort((a, b) => a.cell - b.cell || a.edge.localeCompare(b.edge));
    const value = JSON.stringify({
      version: 2,
      rows: this.#rows,
      columns: this.#columns,
      cells: rows.join("\n"),
      borders,
    });
    this.#value = value;
    this.#internals.setFormValue(value, value);
  }

  #commitValue() {
    this.#updateFormValue();
    this.#updateValidity();
    this.dispatchEvent(new Event("input", { bubbles: true }));
  }

  #updateValidity() {
    if (!this.#cells.length) {
      return;
    }

    const firstFilledCell = this.#cells.find((cell) => cell.value);
    if (this.hasAttribute("required") && !firstFilledCell) {
      this.#internals.setValidity(
        { valueMissing: true },
        "Լրացրեք առնվազն մեկ վանդակ։",
        this.#cells[0],
      );
      return;
    }

    this.#internals.setValidity({});
  }

  #updateDisabledState() {
    const disabled = this.hasAttribute("disabled") || this.#formDisabled;
    this.#cells.forEach((cell) => {
      cell.disabled = disabled;
    });
    this.#thickenBottomButton.disabled = disabled || this.#selectedCells.size === 0;
    this.#thickenLeftButton.disabled = disabled || this.#selectedCells.size === 0;
    this.#clearBordersButton.disabled = disabled || this.#selectedCells.size === 0;
    this.#addRowButton.disabled = disabled;
    this.#removeRowButton.disabled = disabled || this.#rows <= 1;
    this.#addColumnButton.disabled = disabled;
    this.#removeColumnButton.disabled = disabled || this.#columns <= 1;
    this.#updateCellStyles();
  }

  #beginCellSelection(event, index) {
    if (event.button !== 0 || this.hasAttribute("disabled") || this.#formDisabled) {
      return;
    }

    event.preventDefault();
    this.#isDraggingSelection = true;
    if (!event.shiftKey) {
      this.#selectionAnchor = index;
    }
    this.#setActiveCell(index);
    this.#selectRange(this.#selectionAnchor, index);
    this.#focusCell(index);
  }

  #extendCellSelection(index) {
    if (this.#isDraggingSelection && index !== this.#activeCell) {
      this.#setActiveCell(index);
      this.#selectRange(this.#selectionAnchor, index);
    }
  }

  #setActiveCell(index) {
    this.#activeCell = index;
  }

  #selectRange(anchor, active) {
    const anchorRow = Math.floor(anchor / this.#columns);
    const anchorColumn = anchor % this.#columns;
    const activeRow = Math.floor(active / this.#columns);
    const activeColumn = active % this.#columns;
    const minRow = Math.min(anchorRow, activeRow);
    const maxRow = Math.max(anchorRow, activeRow);
    const minColumn = Math.min(anchorColumn, activeColumn);
    const maxColumn = Math.max(anchorColumn, activeColumn);

    this.#selectedCells.clear();
    for (let row = minRow; row <= maxRow; row += 1) {
      for (let column = minColumn; column <= maxColumn; column += 1) {
        this.#selectedCells.add(row * this.#columns + column);
      }
    }
    this.#updateCellStyles();
  }

  #applySelectedBorder(edge) {
    if (this.#selectedCells.size === 0) {
      return;
    }

    const rows = [...this.#selectedCells].map((index) => Math.floor(index / this.#columns));
    const columns = [...this.#selectedCells].map((index) => index % this.#columns);
    const boundary = edge === "bottom" ? Math.max(...rows) : Math.min(...columns);

    for (const index of this.#selectedCells) {
      const row = Math.floor(index / this.#columns);
      const column = index % this.#columns;
      if ((edge === "bottom" && row === boundary) || (edge === "left" && column === boundary)) {
        this.#thickBorders.add(`${index}:${edge}`);
      }
    }

    this.#updateCellStyles();
    this.#commitValue();
  }

  #clearSelectedBorders() {
    for (const index of this.#selectedCells) {
      for (const edge of ["bottom", "left"]) {
        this.#thickBorders.delete(`${index}:${edge}`);
      }
    }
    this.#updateCellStyles();
    this.#commitValue();
  }

  #updateCellStyles() {
    if (!this.#cells.length) {
      return;
    }

    this.#cells.forEach((cell, index) => {
      cell.classList.toggle("selected-cell", this.#selectedCells.has(index));
      cell.classList.toggle("active-cell", index === this.#activeCell);
      for (const edge of ["bottom", "left"]) {
        cell.classList.toggle(`thick-${edge}`, this.#thickBorders.has(`${index}:${edge}`));
      }
    });
    const disabled = this.hasAttribute("disabled") || this.#formDisabled;
    this.#thickenBottomButton.disabled = disabled || this.#selectedCells.size === 0;
    this.#thickenLeftButton.disabled = disabled || this.#selectedCells.size === 0;
    this.#clearBordersButton.disabled = disabled || this.#selectedCells.size === 0;
  }

  #showMessage(message) {
    this.#message.textContent = message;
  }

  #clearMessage() {
    this.#message.textContent = "";
  }

}

if (!customElements.get("math-grid")) {
  customElements.define("math-grid", MathGrid);
}
