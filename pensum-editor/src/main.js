const form = document.querySelector("#workbook-form");
const assignmentList = document.querySelector("#assignment-list");
const status = document.querySelector("#status");
const settingsDialog = document.querySelector("#settings-dialog");
const modeSelect = document.querySelector("#user-mode");
const authorNameInput = document.querySelector("#author-name");
const authorEmailInput = document.querySelector("#author-email");

const USER_MODES = Object.freeze({
  TEACHER: "teacher",
  STUDENT: "student"
});
const USER_MODE_STORAGE_KEY = "pensum-user-mode";
const AUTHOR_DETAILS_STORAGE_KEY = "pensum-author-details";

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

function loadAuthorDetails() {
  try {
    const savedDetails = localStorage.getItem(AUTHOR_DETAILS_STORAGE_KEY);
    if (!savedDetails) return { name: "", email: "" };

    const details = JSON.parse(savedDetails);
    if (
      !details ||
      typeof details.name !== "string" ||
      typeof details.email !== "string"
    ) {
      throw new Error("Պահպանված հեղինակի տվյալները սխալ ձևաչափ ունեն։");
    }
    return { name: details.name, email: details.email };
  } catch (error) {
    showStatus(`Չհաջողվեց կարդալ հեղինակի կարգավորումները։ ${error.message}`, "error");
    return { name: "", email: "" };
  }
}

let userMode = loadUserMode();
let authorDetails = loadAuthorDetails();
let hasUnsavedChanges = false;

function applyUserMode() {
  const isTeacher = userMode === USER_MODES.TEACHER;
  form.hidden = !isTeacher;
  modeSelect.value = userMode;
}

function openSettings() {
  modeSelect.value = userMode;
  authorNameInput.value = authorDetails.name;
  authorEmailInput.value = authorDetails.email;
  if (!settingsDialog.open) settingsDialog.showModal();
}

function getLocalDateString(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

settingsDialog.addEventListener("close", () => {
  if (settingsDialog.returnValue !== "save") return;

  const selectedMode = modeSelect.value;
  if (selectedMode !== USER_MODES.TEACHER && selectedMode !== USER_MODES.STUDENT) {
    showStatus("Ընտրիր վավեր օգտվողի ռեժիմ։", "error");
    return;
  }

  const updatedAuthor = {
    name: authorNameInput.value.trim(),
    email: authorEmailInput.value.trim()
  };
  if (!updatedAuthor.name || !updatedAuthor.email || !authorEmailInput.checkValidity()) {
    showStatus("Լրացրու հեղինակի անունն ու վավեր էլ․ փոստը։", "error");
    return;
  }

  try {
    localStorage.setItem(USER_MODE_STORAGE_KEY, selectedMode);
    localStorage.setItem(AUTHOR_DETAILS_STORAGE_KEY, JSON.stringify(updatedAuthor));
    userMode = selectedMode;
    authorDetails = updatedAuthor;
    applyUserMode();
    showStatus("Կարգավորումները պահպանվեցին։");
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

async function confirmDiscardChanges(message) {
  if (!hasUnsavedChanges) return true;
  return window.__TAURI__.dialog.confirm(
    `Չպահպանված փոփոխությունները կկորչեն։ ${message}`,
    { title: "Չպահպանված փոփոխություններ", kind: "warning" }
  );
}

async function createWorkbook() {
  if (userMode !== USER_MODES.TEACHER) return;

  try {
    if (!(await confirmDiscardChanges("Ստեղծե՞լ նոր դատարկ տետր։"))) return;

    form.elements.title.value = "";
    assignmentList.replaceChildren();
    hasUnsavedChanges = false;
    showStatus("Ստեղծվեց նոր դատարկ տետր։", "success");
  } catch (error) {
    showStatus(`Չհաջողվեց ստեղծել նոր տետրը։ ${error.message}`, "error");
  }
}

async function listenForCreateMenu() {
  try {
    await window.__TAURI__.event.listen("create-workbook", createWorkbook);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել ստեղծման մենյուն։ ${error.message}`, "error");
  }
}

async function saveWorkbook() {
  if (userMode !== USER_MODES.TEACHER || !form.reportValidity()) return;
  if (!authorDetails.name.trim() || !authorDetails.email.trim()) {
    showStatus("Կարգավորումներում լրացրու հեղինակի անունն ու էլ․ փոստը։", "error");
    return;
  }
  const emailValidator = document.createElement("input");
  emailValidator.type = "email";
  emailValidator.value = authorDetails.email;
  if (!emailValidator.checkValidity()) {
    showStatus("Կարգավորումներում լրացրու վավեր էլ․ փոստ։", "error");
    return;
  }

  try {
    const documentData = createDocument();
    const safeName = documentData.title.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-") || "workbook";
    const path = await window.__TAURI__.dialog.save({
      title: "Պահպանել տետրը",
      defaultPath: `${safeName}.pdw`,
      filters: [{ name: "PDW տետր (ZIP)", extensions: ["pdw"] }]
    });
    if (!path) return;

    documentData["created-at"] = getLocalDateString();
    await window.__TAURI__.core.invoke("save_workbook", {
      path,
      workbook: documentData
    });
    hasUnsavedChanges = false;
    showStatus("Տետրը պահպանվեց PDW ֆայլով։", "success");
  } catch (error) {
    showStatus(`Չհաջողվեց պահպանել տետրը։ ${error.message}`, "error");
  }
}

async function listenForSaveMenu() {
  try {
    await window.__TAURI__.event.listen("save-workbook", saveWorkbook);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել պահպանման մենյուն։ ${error.message}`, "error");
  }
}

async function openWorkbook() {
  if (userMode !== USER_MODES.TEACHER) return;

  try {
    const path = await window.__TAURI__.dialog.open({
      title: "Բացել տետրը",
      multiple: false,
      directory: false,
      filters: [{ name: "PDW տետր (ZIP)", extensions: ["pdw"] }]
    });
    if (!path) return;

    if (!(await confirmDiscardChanges("Բացե՞լ ընտրված տետրը։"))) return;

    const documentData = await window.__TAURI__.core.invoke("open_workbook", { path });
    loadWorkbookDocument(documentData);
  } catch (error) {
    showStatus(`Չհաջողվեց բացել տետրը։ ${error.message}`, "error");
  }
}

async function listenForOpenMenu() {
  try {
    await window.__TAURI__.event.listen("open-workbook", openWorkbook);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել բացման մենյուն։ ${error.message}`, "error");
  }
}

async function exitWorkbook() {
  try {
    if (!(await confirmDiscardChanges("Դո՞ւրս գալ ծրագրից։"))) return;
    await window.__TAURI__.core.invoke("exit_app");
  } catch (error) {
    showStatus(`Չհաջողվեց փակել ծրագիրը։ ${error.message}`, "error");
  }
}

async function listenForExitMenu() {
  try {
    await window.__TAURI__.event.listen("exit-workbook", exitWorkbook);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել ելքի մենյուն։ ${error.message}`, "error");
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

function createSquaredEditor(initialContent) {
  const editor = document.createElement("div");
  const scroll = document.createElement("div");
  scroll.className = "grid-scroll";
  const grid = document.createElement("div");
  grid.className = "grid-editor";
  grid.style.display = "grid";
  grid.style.width = "max-content";
  grid.style.borderTop = "1px solid #cbd4e2";
  grid.style.borderLeft = "1px solid #cbd4e2";
  const savedRows = Array.isArray(initialContent) && initialContent.length > 0
    ? initialContent
    : null;
  let columnCount = savedRows
    ? savedRows.reduce((count, row) => Math.max(count, row.length), 1)
    : 20;

  function addRow(values = []) {
    const row = document.createElement("div");
    row.className = "grid-row";
    row.setAttribute("role", "row");
    for (let column = 0; column < columnCount; column += 1) {
      const cell = createGridCell(grid.children.length, column);
      cell.value = values[column] ?? "";
      cell.setAttribute("role", "gridcell");
      row.append(cell);
    }
    grid.append(row);
  }

  grid.style.gridTemplateColumns = `repeat(${columnCount}, 6mm)`;
  grid.setAttribute("role", "grid");
  if (savedRows) {
    savedRows.forEach(values => addRow(values));
  } else {
    for (let row = 0; row < 15; row += 1) addRow();
  }
  scroll.append(grid);

  const tools = document.createElement("div");
  tools.className = "grid-tools";
  tools.append(
    makeIconButton("rows-3-plus", "button button-secondary grid-tool-button", () => {
      addRow();
      clearOutput();
    }),
    makeIconButton("rows-3-minus", "button button-secondary grid-tool-button", () => {
      if (grid.children.length > 1) {
        grid.lastElementChild.remove();
        clearOutput();
      }
    }),
    makeIconButton("columns-3-plus", "button button-secondary grid-tool-button", () => {
      columnCount += 1;
      grid.style.gridTemplateColumns = `repeat(${columnCount}, 6mm)`;
      grid.querySelectorAll(".grid-row").forEach((row, rowIndex) => {
        const cell = createGridCell(rowIndex, row.children.length);
        cell.setAttribute("role", "gridcell");
        row.append(cell);
      });
      clearOutput();
    }),
    makeIconButton("columns-3-minus", "button button-secondary grid-tool-button", () => {
      const rows = [...grid.querySelectorAll(".grid-row")];
      if (columnCount > 1) {
        columnCount -= 1;
        grid.style.gridTemplateColumns = `repeat(${columnCount}, 6mm)`;
        rows.forEach(row => row.lastElementChild.remove());
        clearOutput();
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

function createContentBlock(type, index, imageUrl, initialContent = "") {
  const block = document.createElement("div");
  block.className = "content-block";
  block.dataset.type = type;

  const removeButton = makeButton("×", "button button-danger remove-block-button", () => {
    block.remove();
    clearOutput();
  });
  removeButton.setAttribute("aria-label", "Հեռացնել բլոկը");
  removeButton.title = "Հեռացնել";

  if (type === "text") {
    block.classList.add("content-block-text");
    const textarea = document.createElement("textarea");
    textarea.setAttribute("aria-label", "Տեքստի բովանդակություն");
    textarea.placeholder = "Տեքստ";
    textarea.value = initialContent;
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
  block.append(createSquaredEditor(initialContent), removeButton);
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

function createContentSection(name, title, initialBlocks = []) {
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
  initialBlocks.forEach((savedBlock, index) => {
    let block;
    if (savedBlock.type === "text") {
      block = createContentBlock("text", index, undefined, savedBlock.content);
    } else if (savedBlock.type === "image") {
      block = createContentBlock("image", index, savedBlock.url);
    } else {
      block = createContentBlock("squared", index, undefined, savedBlock.content);
    }
    blocks.append(block);
  });
  const tools = document.createElement("div");
  tools.className = "add-block-tools";
  tools.append(
    makeButton("", "add-block-button add-block-button-text", () => {
      blocks.append(createContentBlock("text", blocks.children.length));
      clearOutput();
    }),
    makeButton("", "add-block-button add-block-button-image", () => openImagePicker(blocks)),
    makeButton("", "add-block-button add-block-button-grid", () => {
      blocks.append(createContentBlock("squared", blocks.children.length));
      clearOutput();
    })
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

function createAssignment(savedAssignment = { problem: [] }) {
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
  removeButton.addEventListener("click", clearOutput);

  const sections = document.createElement("div");
  sections.className = "assignment-sections";
  sections.append(
    createContentSection("problem", "Խնդիր", savedAssignment.problem),
  );
  card.append(heading, sections);
  assignmentList.append(card);
  updateAssignmentNumbers();
}

function clearOutput(message = "Փոփոխություններ կան․ տետրը պահպանելու համար ընտրիր «Պահպանել»։") {
  hasUnsavedChanges = true;
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
    author: { ...authorDetails },
    "created-at": getLocalDateString(),
    assignments
  };
}

function validateWorkbookDocument(documentData) {
  if (!documentData || typeof documentData !== "object" || Array.isArray(documentData)) {
    throw new Error("Տետրի տվյալները սխալ ձևաչափ ունեն։");
  }
  if (documentData.formatVersion !== 1) {
    throw new Error("Տետրի ZIP ձևաչափի տարբերակը չի աջակցվում։");
  }
  if (
    typeof documentData.title !== "string" ||
    !(
      typeof documentData.author === "string" ||
      (
        documentData.author &&
        typeof documentData.author === "object" &&
        typeof documentData.author.name === "string" &&
        typeof documentData.author.email === "string"
      )
    ) ||
    typeof documentData["created-at"] !== "string"
  ) {
    throw new Error("Տետրի հիմնական տվյալները սխալ ձևաչափ ունեն։");
  }
  if (!Array.isArray(documentData.assignments)) {
    throw new Error("Տետրում առաջադրանքների ցուցակ չկա։");
  }
  for (const assignment of documentData.assignments) {
    if (!assignment || typeof assignment !== "object" || !Array.isArray(assignment.problem)) {
      throw new Error("Առաջադրանքի տվյալները սխալ ձևաչափ ունեն։");
    }
    for (const block of assignment.problem) {
      if (!block || typeof block !== "object") {
        throw new Error("Բովանդակության բլոկի տվյալները սխալ ձևաչափ ունեն։");
      }
      if (block.type === "text" && typeof block.content === "string") continue;
      if (block.type === "image" && typeof block.url === "string" && block.url.startsWith("data:image/")) continue;
      if (
        block.type === "squared" &&
        Array.isArray(block.content) &&
        block.content.every(row => Array.isArray(row) && row.every(cell => typeof cell === "string"))
      ) continue;
      throw new Error("Բովանդակության բլոկի տեսակը կամ տվյալները չեն աջակցվում։");
    }
  }
}

function loadWorkbookDocument(documentData) {
  validateWorkbookDocument(documentData);
  form.elements.title.value = documentData.title;
  assignmentList.replaceChildren();
  documentData.assignments.forEach(createAssignment);
  hasUnsavedChanges = false;
  showStatus("Կիսատ տետրը բացվեց։ Կարող ես շարունակել խմբագրումը։", "success");
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
  saveWorkbook();
});

applyUserMode();
listenForCreateMenu();
listenForSettingsMenu();
listenForSaveMenu();
listenForOpenMenu();
listenForExitMenu();
