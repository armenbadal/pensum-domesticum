const form = document.querySelector("#workbook-form");
const assignmentList = document.querySelector("#assignment-list");
const status = document.querySelector("#status");
const settingsDialog = document.querySelector("#settings-dialog");
const modeSelect = document.querySelector("#user-mode");

const USER_MODES = Object.freeze({
  TEACHER: "teacher",
  STUDENT: "student"
});
const USER_MODE_STORAGE_KEY = "pensum-user-mode";

function showStatus(message, type = "") {
  status.textContent = message;
  status.className = type ? `status status-${type}` : "status";
}

function loadUserMode() {
  try {
    const savedMode = localStorage.getItem(USER_MODE_STORAGE_KEY);
    return savedMode === USER_MODES.STUDENT ? USER_MODES.STUDENT : USER_MODES.TEACHER;
  } catch (error) {
    showStatus(`Չհաջողվեց կարդալ պահպանված ռեժիմը։ ${error.message}`, "error");
    return USER_MODES.TEACHER;
  }
}

let userMode = loadUserMode();

function applyUserMode() {
  const isTeacher = userMode === USER_MODES.TEACHER;
  form.hidden = !isTeacher;
  modeSelect.value = userMode;

  if (isTeacher && assignmentList.children.length === 0) {
    createAssignment();
  }
}

function openSettings() {
  modeSelect.value = userMode;
  if (!settingsDialog.open) settingsDialog.showModal();
}

settingsDialog.addEventListener("close", () => {
  if (settingsDialog.returnValue !== "save") return;

  const selectedMode = modeSelect.value;
  if (selectedMode !== USER_MODES.TEACHER && selectedMode !== USER_MODES.STUDENT) {
    showStatus("Ընտրիր վավեր օգտվողի ռեժիմ։", "error");
    return;
  }

  try {
    localStorage.setItem(USER_MODE_STORAGE_KEY, selectedMode);
    userMode = selectedMode;
    applyUserMode();
    clearOutput(`Պահպանվեց ${userMode === USER_MODES.TEACHER ? "դասատուի" : "աշակերտի"} ռեժիմը։`);
  } catch (error) {
    showStatus(`Չհաջողվեց պահպանել ռեժիմը։ ${error.message}`, "error");
  }
});

async function listenForSettingsMenu() {
  try {
    await window.__TAURI__.event.listen("open-settings", openSettings);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել կարգավորումների մենյուն։ ${error.message}`, "error");
  }
}

function makeButton(text, className, onClick, type = "button") {
  const button = document.createElement("button");
  button.type = type;
  button.className = className;
  button.textContent = text;
  if (onClick) button.addEventListener("click", onClick);
  return button;
}

function makeIconButton(iconName, className, onClick) {
  const button = makeButton("", className, onClick);
  const icon = document.createElement("img");
  icon.className = "grid-tool-icon";
  icon.src = `./assets/icons/${iconName}.svg`;
  icon.alt = "";
  icon.draggable = false;
  button.append(icon);
  return button;
}

function updateAssignmentNumbers() {
  assignmentList.querySelectorAll(".assignment-card").forEach((card, index) => {
    card.querySelector(".assignment-number").textContent = `Առաջադրանք ${index + 1}`;
  });
}

function createGridCell(rowIndex, columnIndex) {
  const input = document.createElement("input");
  input.type = "text";
  input.maxLength = 1;
  input.autocomplete = "off";
  input.setAttribute("aria-label", `Տող ${rowIndex + 1}, վանդակ ${columnIndex + 1}`);
  input.addEventListener("input", () => {
    if (!input.value) return;
    const cells = [...input.closest(".grid-editor").querySelectorAll("input")];
    cells[cells.indexOf(input) + 1]?.focus();
  });
  return input;
}

function createSquaredEditor() {
  const editor = document.createElement("div");
  const scroll = document.createElement("div");
  scroll.className = "grid-scroll";
  const grid = document.createElement("div");
  grid.className = "grid-editor";
  grid.style.display = "grid";
  grid.style.width = "max-content";
  grid.style.borderTop = "1px solid #cbd4e2";
  grid.style.borderLeft = "1px solid #cbd4e2";
  let columnCount = 20;

  function addRow(values = []) {
    const row = document.createElement("div");
    row.className = "grid-row";
    row.setAttribute("role", "row");
    for (let column = 0; column < columnCount; column += 1) {
      const cell = createGridCell(grid.children.length, column);
      cell.value = values[column] || "";
      cell.setAttribute("role", "gridcell");
      row.append(cell);
    }
    grid.append(row);
  }

  grid.style.gridTemplateColumns = `repeat(${columnCount}, 6mm)`;
  grid.setAttribute("role", "grid");
  for (let row = 0; row < 15; row += 1) addRow();
  scroll.append(grid);

  const tools = document.createElement("div");
  tools.className = "grid-tools";
  tools.append(
    makeIconButton("rows-3-plus", "button button-secondary grid-tool-button", () => addRow()),
    makeIconButton("rows-3-minus", "button button-secondary grid-tool-button", () => {
      if (grid.children.length > 1) grid.lastElementChild.remove();
    }),
    makeIconButton("columns-3-plus", "button button-secondary grid-tool-button", () => {
      columnCount += 1;
      grid.style.gridTemplateColumns = `repeat(${columnCount}, 6mm)`;
      grid.querySelectorAll(".grid-row").forEach((row, rowIndex) => {
        const cell = createGridCell(rowIndex, row.children.length);
        cell.setAttribute("role", "gridcell");
        row.append(cell);
      });
    }),
    makeIconButton("columns-3-minus", "button button-secondary grid-tool-button", () => {
      const rows = [...grid.querySelectorAll(".grid-row")];
      if (columnCount > 1) {
        columnCount -= 1;
        grid.style.gridTemplateColumns = `repeat(${columnCount}, 6mm)`;
        rows.forEach(row => row.lastElementChild.remove());
      }
    })
  );
  ["Ավելացնել տող", "Հեռացնել վերջին տողը", "Ավելացնել սյուն", "Հեռացնել վերջին սյունը"]
    .forEach((label, index) => {
      tools.children[index].setAttribute("aria-label", label);
      tools.children[index].title = label;
    });
  editor.append(scroll, tools);
  return editor;
}

function createContentBlock(type, index, imageUrl) {
  const block = document.createElement("div");
  block.className = "content-block";
  block.dataset.type = type;

  const removeButton = makeButton("×", "button button-danger remove-block-button", () => block.remove());
  removeButton.setAttribute("aria-label", "Հեռացնել բլոկը");
  removeButton.title = "Հեռացնել";

  if (type === "text") {
    block.classList.add("content-block-text");
    const textarea = document.createElement("textarea");
    textarea.setAttribute("aria-label", "Տեքստի բովանդակություն");
    textarea.placeholder = "Տեքստ";
    block.append(textarea, removeButton);
    return block;
  }

  if (type === "image") {
    block.classList.add("content-block-image");
    block.dataset.url = imageUrl;
    const image = document.createElement("img");
    image.className = "content-block-image-preview";
    image.src = imageUrl;
    image.alt = "Ընտրված պատկեր";
    block.append(image, removeButton);
    return block;
  }

  block.classList.add("content-block-squared");
  removeButton.classList.add("content-block-squared-remove");
  block.append(createSquaredEditor(), removeButton);
  return block;
}

function openImagePicker(blocks) {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = "image/*";
  input.hidden = true;
  input.setAttribute("aria-label", "Ընտրել պատկեր");
  document.body.append(input);

  input.addEventListener("cancel", () => input.remove(), { once: true });
  input.addEventListener("change", () => {
    const file = input.files[0];
    input.remove();
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      status.textContent = "Ընտրիր պատկերի ֆայլ։";
      status.className = "status status-error";
      return;
    }

    const reader = new FileReader();
    reader.addEventListener("load", () => {
      if (typeof reader.result !== "string") {
        status.textContent = "Չհաջողվեց կարդալ ընտրված պատկերը։";
        status.className = "status status-error";
        return;
      }
      blocks.append(createContentBlock("image", blocks.children.length, reader.result));
      clearOutput();
    });
    reader.addEventListener("error", () => {
      status.textContent = "Չհաջողվեց կարդալ ընտրված պատկերը։";
      status.className = "status status-error";
    });
    reader.readAsDataURL(file);
  }, { once: true });
  input.click();
}

function createContentSection(name, title) {
  const section = document.createElement("section");
  section.className = "content-section";
  section.dataset.section = name;

  const heading = document.createElement("div");
  heading.className = "section-heading";
  const titleElement = document.createElement("h4");
  titleElement.textContent = title;
  heading.append(titleElement);

  const blocks = document.createElement("div");
  blocks.className = "block-list";
  const tools = document.createElement("div");
  tools.className = "add-block-tools";
  tools.append(
    makeButton("", "add-block-button add-block-button-text", () => blocks.append(createContentBlock("text", blocks.children.length))),
    makeButton("", "add-block-button add-block-button-image", () => openImagePicker(blocks)),
    makeButton("", "add-block-button add-block-button-grid", () => blocks.append(createContentBlock("squared", blocks.children.length)))
  );
  tools.children[0].setAttribute("aria-label", "Ավելացնել տեքստային բլոկ");
  tools.children[0].title = "Տեքստ";
  tools.children[1].setAttribute("aria-label", "Ավելացնել պատկերի բլոկ");
  tools.children[1].title = "Պատկեր";
  tools.children[2].setAttribute("aria-label", "Ավելացնել վանդակավոր դաշտ");
  tools.children[2].title = "Վանդակավոր դաշտ";
  section.append(heading, blocks, tools);
  return section;
}

function createAssignment() {
  const card = document.createElement("article");
  card.className = "assignment-card";

  const heading = document.createElement("div");
  heading.className = "assignment-heading";
  const title = document.createElement("h4");
  title.className = "assignment-number";
  const removeButton = makeButton("×", "button button-danger remove-block-button", () => {
    card.remove();
    updateAssignmentNumbers();
  });
  removeButton.setAttribute("aria-label", "Հեռացնել առաջադրանքը");
  removeButton.title = "Հեռացնել առաջադրանքը";
  heading.append(
    title,
    removeButton
  );

  const sections = document.createElement("div");
  sections.className = "assignment-sections";
  sections.append(
    createContentSection("problem", "Խնդիր"),
  );
  card.append(heading, sections);
  assignmentList.append(card);
  updateAssignmentNumbers();
}

function clearOutput(message = "Փոփոխություններ կան․ ստեղծիր և ներբեռնիր JSON-ը։") {
  showStatus(message);
}

function collectBlocks(section) {
  return [...section.querySelector(".block-list").children].map(block => {
    const type = block.dataset.type;
    if (type === "text") {
      return { type, content: block.querySelector("textarea").value };
    }
    if (type === "image") {
      return { type, url: block.dataset.url };
    }
    const content = [...block.querySelectorAll(".grid-row")].map(row =>
      [...row.querySelectorAll("input")].map(cell => cell.value)
    );
    return { type, content };
  });
}

function createDocument() {
  const assignments = [...assignmentList.querySelectorAll(".assignment-card")].map(card => {
    const problemSection = card.querySelector('[data-section="problem"]');
    return {
      problem: collectBlocks(problemSection),
    };
  });

  return {
    title: form.elements.title.value,
    author: form.elements.author.value,
    "created-at": form.elements["created-at"].value,
    assignments
  };
}

form.addEventListener("input", () => clearOutput());
form.addEventListener("change", () => clearOutput());

document.querySelector("#add-assignment").addEventListener("click", () => {
  if (userMode !== USER_MODES.TEACHER) return;
  createAssignment();
  clearOutput();
});

form.addEventListener("submit", event => {
  event.preventDefault();
  if (userMode !== USER_MODES.TEACHER) return;
  try {
    const documentData = createDocument();
    const file = new Blob([JSON.stringify(documentData, null, 2)], {
      type: "application/json;charset=utf-8"
    });
    const link = document.createElement("a");
    const safeName = documentData.title.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-") || "workbook";
    link.href = URL.createObjectURL(file);
    link.download = `${safeName}.json`;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(link.href), 1000);
    showStatus("JSON փաստաթուղթը ստեղծվեց և ներբեռնվեց։", "success");
  } catch (error) {
    showStatus(error.message, "error");
  }
});

const localDate = new Date();
const year = localDate.getFullYear();
const month = String(localDate.getMonth() + 1).padStart(2, "0");
const day = String(localDate.getDate()).padStart(2, "0");
form.elements["created-at"].value = `${year}-${month}-${day}`;
createAssignment();
applyUserMode();
listenForSettingsMenu();
