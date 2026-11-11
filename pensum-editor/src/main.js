import "./components/math-grid/math-grid.js";

const form = document.querySelector("#workbook-form");
const assignmentList = document.querySelector("#assignment-list");
const status = document.querySelector("#status");
const settingsDialog = document.querySelector("#settings-dialog");
const modeSelect = document.querySelector("#user-mode");
const profileNameLabel = document.querySelector("#profile-name-label");
const profileEmailLabel = document.querySelector("#profile-email-label");
const profileNameInput = document.querySelector("#profile-name");
const profileEmailInput = document.querySelector("#profile-email");
const addAssignmentButton = document.querySelector("#add-assignment");

const USER_MODES = Object.freeze({
  TEACHER: "teacher",
  STUDENT: "student"
});
const USER_MODE_STORAGE_KEY = "pensum-user-mode";
const AUTHOR_DETAILS_STORAGE_KEY = "pensum-author-details";
const STUDENT_DETAILS_STORAGE_KEY = "pensum-student-details";

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

function loadProfileDetails(storageKey, profileName) {
  try {
    const savedDetails = localStorage.getItem(storageKey);
    if (!savedDetails) return { name: "", email: "" };

    const details = JSON.parse(savedDetails);
    if (
      !details ||
      typeof details.name !== "string" ||
      typeof details.email !== "string"
    ) {
      throw new Error(`Պահպանված ${profileName} տվյալները սխալ ձևաչափ ունեն։`);
    }
    return { name: details.name, email: details.email };
  } catch (error) {
    showStatus(`Չհաջողվեց կարդալ ${profileName} կարգավորումները։ ${error.message}`, "error");
    return { name: "", email: "" };
  }
}

let userMode = loadUserMode();
let authorDetails = loadProfileDetails(AUTHOR_DETAILS_STORAGE_KEY, "հեղինակի");
let studentDetails = loadProfileDetails(STUDENT_DETAILS_STORAGE_KEY, "աշակերտի");
let hasUnsavedChanges = false;
let hasLoadedWorkbook = false;
let workbookSolved = false;
let workbookAuthor = null;
const assignmentSolutions = new WeakMap();

function applyUserMode(assignments = collectAssignments()) {
  const isTeacher = userMode === USER_MODES.TEACHER;
  const canEditTeacherContent = isTeacher && !workbookSolved;
  form.hidden = false;
  form.elements.title.readOnly = !canEditTeacherContent;
  addAssignmentButton.hidden = !canEditTeacherContent;
  modeSelect.value = userMode;
  assignmentList.replaceChildren();
  assignments.forEach(createAssignment);
  updateSettingsProfileFields();
}

function updateSettingsProfileFields() {
  const isTeacher = modeSelect.value === USER_MODES.TEACHER;
  const details = isTeacher ? authorDetails : studentDetails;
  profileNameLabel.textContent = isTeacher ? "Հեղինակի անուն" : "Աշակերտի անուն";
  profileEmailLabel.textContent = isTeacher ? "Հեղինակի էլ․ փոստ" : "Աշակերտի էլ․ փոստ";
  profileNameInput.value = details.name;
  profileEmailInput.value = details.email;
}

function openSettings() {
  modeSelect.value = userMode;
  updateSettingsProfileFields();
  if (!settingsDialog.open) settingsDialog.showModal();
}

function getLocalDateString(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

modeSelect.addEventListener("change", updateSettingsProfileFields);

settingsDialog.addEventListener("close", async () => {
  if (settingsDialog.returnValue !== "save") return;

  const selectedMode = modeSelect.value;
  if (selectedMode !== USER_MODES.TEACHER && selectedMode !== USER_MODES.STUDENT) {
    showStatus("Ընտրիր վավեր օգտվողի ռեժիմ։", "error");
    return;
  }

  const isTeacher = selectedMode === USER_MODES.TEACHER;
  const updatedDetails = {
    name: profileNameInput.value.trim(),
    email: profileEmailInput.value.trim()
  };
  if (!updatedDetails.name || !updatedDetails.email || !profileEmailInput.checkValidity()) {
    showStatus(
      isTeacher
        ? "Լրացրու հեղինակի անունն ու վավեր էլ․ փոստը։"
        : "Լրացրու աշակերտի անունն ու վավեր էլ․ փոստը։",
      "error"
    );
    return;
  }

  try {
    localStorage.setItem(USER_MODE_STORAGE_KEY, selectedMode);
    const detailsStorageKey = isTeacher ? AUTHOR_DETAILS_STORAGE_KEY : STUDENT_DETAILS_STORAGE_KEY;
    localStorage.setItem(detailsStorageKey, JSON.stringify(updatedDetails));
    await window.__TAURI__.core.invoke("set_user_mode", {
      mode: selectedMode,
      solved: workbookSolved
    });
    const assignments = collectAssignments();
    userMode = selectedMode;
    if (isTeacher) {
      authorDetails = updatedDetails;
    } else {
      studentDetails = updatedDetails;
    }
    applyUserMode(assignments);
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

async function syncNativeMenuMode() {
  try {
    await window.__TAURI__.core.invoke("set_user_mode", {
      mode: userMode,
      solved: workbookSolved
    });
  } catch (error) {
    showStatus(`Չհաջողվեց կիրառել օգտվողի ռեժիմի մենյուն։ ${error.message}`, "error");
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
    hasLoadedWorkbook = false;
    workbookSolved = false;
    workbookAuthor = null;
    hasUnsavedChanges = false;
    await syncNativeMenuMode();
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
  if (userMode === USER_MODES.TEACHER && workbookSolved) {
    showStatus("Աշակերտի լուծած տետրը միայն դիտման համար է։", "error");
    return;
  }
  if (userMode === USER_MODES.STUDENT && !hasLoadedWorkbook) {
    showStatus("Պահպանելու համար նախ բացիր ուսուցչի տետրը։", "error");
    return;
  }
  if (!form.reportValidity()) return;
  if (userMode === USER_MODES.TEACHER) {
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
  }

  try {
    const documentData = createDocument();
    const safeName = documentData.title.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-") || "workbook";
    const path = await window.__TAURI__.dialog.save({
      title: "Պահպանել տետրը",
      defaultPath: `${safeName}${userMode === USER_MODES.STUDENT ? "-response" : ""}.pdw`,
      filters: [{ name: "PDW տետր (ZIP)", extensions: ["pdw"] }]
    });
    if (!path) return;

    documentData["created-at"] = getLocalDateString();
    await window.__TAURI__.core.invoke("save_workbook", {
      path,
      workbook: documentData
    });
    workbookSolved = documentData.solved;
    hasUnsavedChanges = false;
    await syncNativeMenuMode();
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
    await syncNativeMenuMode();
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

function updateAssignmentNumbers() {
  assignmentList.querySelectorAll(".assignment-card").forEach((card, index) => {
    card.querySelector(".assignment-number").textContent = `Առաջադրանք ${index + 1}`;
  });
}

function toMathGridState(content) {
  if (!Array.isArray(content)) return content;

  const rows = content.length || 10;
  const columns = content.length
    ? Math.max(...content.map(row => row.length), 1)
    : 20;
  return {
    version: 2,
    rows,
    columns,
    cells: content.length
      ? content.map(row => Array.from({ length: columns }, (_, index) => row[index] || " ").join("")).join("\n")
      : Array.from({ length: rows }, () => " ".repeat(columns)).join("\n"),
    borders: []
  };
}

function isValidSquaredContent(content) {
  if (Array.isArray(content)) {
    return content.every(row => Array.isArray(row) && row.every(cell => {
      if (typeof cell !== "string") return false;
      return cell === "" || ([...cell].length === 1 && /^[0-9+\-−*/×÷=()]$/u.test(cell));
    }));
  }
  if (!content || typeof content !== "object" || Array.isArray(content)) return false;

  const rows = content.version === 1 ? 20 : content.rows;
  const columns = content.version === 1 ? 20 : content.columns;
  if (
    ![1, 2].includes(content.version) ||
    !Number.isSafeInteger(rows) ||
    rows < 1 ||
    !Number.isSafeInteger(columns) ||
    columns < 1 ||
    typeof content.cells !== "string" ||
    !Array.isArray(content.borders)
  ) {
    return false;
  }

  const gridRows = content.cells.split("\n");
  return gridRows.length === rows &&
    gridRows.every(row => [...row].length === columns &&
      [...row].every(character => character === " " || /^[0-9+\-−*/×÷=()]$/u.test(character))) &&
    content.borders.every(border => border &&
      Number.isInteger(border.cell) &&
      border.cell >= 0 &&
      border.cell < rows * columns &&
      ["bottom", "left"].includes(border.edge));
}

function createSquaredEditor(initialContent, editable = true) {
  const scroll = document.createElement("div");
  scroll.className = "grid-scroll";
  const grid = document.createElement("math-grid");
  grid.setAttribute("aria-label", "Վանդակավոր դաշտ");
  grid.toggleAttribute("disabled", !editable);
  if (Array.isArray(initialContent) || (initialContent && typeof initialContent === "object")) {
    grid.value = JSON.stringify(toMathGridState(initialContent));
  }
  grid.addEventListener("input", clearOutput);
  scroll.append(grid);
  return scroll;
}

function createContentBlock(type, index, imageUrl, initialContent = "", editable = true) {
  const block = document.createElement("div");
  block.className = "content-block";
  block.dataset.type = type;

  const removeButton = makeButton("×", "button button-danger remove-block-button", () => {
    block.remove();
    clearOutput();
  });
  removeButton.hidden = !editable;
  removeButton.setAttribute("aria-label", "Հեռացնել բլոկը");
  removeButton.title = "Հեռացնել";

  if (type === "text") {
    block.classList.add("content-block-text");
    const textarea = document.createElement("textarea");
    textarea.setAttribute("aria-label", "Տեքստի բովանդակություն");
    textarea.placeholder = "Տեքստ";
    textarea.value = initialContent;
    textarea.readOnly = !editable;
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
  block.append(createSquaredEditor(initialContent, editable), removeButton);
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

function createContentSection(name, title, initialBlocks = [], editable = true) {
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
      block = createContentBlock("text", index, undefined, savedBlock.content, editable);
    } else if (savedBlock.type === "image") {
      block = createContentBlock("image", index, savedBlock.url, "", editable);
    } else {
      block = createContentBlock("squared", index, undefined, savedBlock.content, editable);
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
  tools.hidden = !editable;
  section.append(heading, blocks, tools);
  return section;
}

function createAssignment(savedAssignment = { problem: [] }) {
  const card = document.createElement("article");
  card.className = "assignment-card";
  const isTeacher = userMode === USER_MODES.TEACHER;
  const canEditTeacherContent = isTeacher && !workbookSolved;
  const savedSolution = Array.isArray(savedAssignment.solution) ? savedAssignment.solution : [];
  assignmentSolutions.set(card, savedSolution);

  const heading = document.createElement("div");
  heading.className = "assignment-heading";
  const title = document.createElement("h4");
  title.className = "assignment-number";
  const removeButton = makeButton("×", "button button-danger remove-block-button", () => {
    card.remove();
    updateAssignmentNumbers();
  });
  removeButton.hidden = !canEditTeacherContent;
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
    createContentSection("problem", "Խնդիր", savedAssignment.problem, canEditTeacherContent),
  );
  if (!isTeacher || savedSolution.length > 0) {
    sections.append(
      createContentSection("solution", "Լուծում", savedSolution, !isTeacher)
    );
  }
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
    const content = JSON.parse(block.querySelector("math-grid").value);
    return { type, content };
  });
}

function collectAssignments() {
  return [...assignmentList.querySelectorAll(".assignment-card")].map(card => {
    const problemSection = card.querySelector('[data-section="problem"]');
    const solutionSection = card.querySelector('[data-section="solution"]');
    return {
      problem: collectBlocks(problemSection),
      solution: solutionSection
        ? collectBlocks(solutionSection)
        : assignmentSolutions.get(card) ?? []
    };
  });
}

function createDocument() {
  return {
    title: form.elements.title.value,
    author: userMode === USER_MODES.STUDENT ? workbookAuthor : { ...authorDetails },
    "created-at": getLocalDateString(),
    solved: userMode === USER_MODES.STUDENT || workbookSolved,
    assignments: collectAssignments()
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
    (documentData.solved !== undefined && typeof documentData.solved !== "boolean") ||
    typeof documentData["created-at"] !== "string"
  ) {
    throw new Error("Տետրի հիմնական տվյալները սխալ ձևաչափ ունեն։");
  }
  if (!Array.isArray(documentData.assignments)) {
    throw new Error("Տետրում առաջադրանքների ցուցակ չկա։");
  }
  for (const assignment of documentData.assignments) {
    if (
      !assignment ||
      typeof assignment !== "object" ||
      !Array.isArray(assignment.problem) ||
      (assignment.solution !== undefined && !Array.isArray(assignment.solution))
    ) {
      throw new Error("Առաջադրանքի տվյալները սխալ ձևաչափ ունեն։");
    }
    for (const blocks of [assignment.problem, assignment.solution ?? []]) {
      for (const block of blocks) {
        if (!block || typeof block !== "object") {
          throw new Error("Բովանդակության բլոկի տվյալները սխալ ձևաչափ ունեն։");
        }
        if (block.type === "text" && typeof block.content === "string") continue;
        if (block.type === "image" && typeof block.url === "string" && block.url.startsWith("data:image/")) continue;
        if (block.type === "squared" && isValidSquaredContent(block.content)) continue;
        throw new Error("Բովանդակության բլոկի տեսակը կամ տվյալները չեն աջակցվում։");
      }
    }
  }
}

function loadWorkbookDocument(documentData) {
  validateWorkbookDocument(documentData);
  form.elements.title.value = documentData.title;
  workbookAuthor = documentData.author;
  hasLoadedWorkbook = true;
  workbookSolved = documentData.solved === true;
  assignmentList.replaceChildren();
  documentData.assignments.forEach(createAssignment);
  hasUnsavedChanges = false;
  showStatus(
    workbookSolved && userMode === USER_MODES.TEACHER
      ? "Աշակերտի լուծած տետրը բացվեց միայն դիտման համար։"
      : "Կիսատ տետրը բացվեց։ Կարող ես շարունակել խմբագրումը։",
    "success"
  );
}

form.addEventListener("input", () => clearOutput());
form.addEventListener("change", () => clearOutput());

document.querySelector("#add-assignment").addEventListener("click", () => {
  if (userMode !== USER_MODES.TEACHER || workbookSolved) return;
  createAssignment();
  clearOutput();
});

form.addEventListener("submit", event => {
  event.preventDefault();
  saveWorkbook();
});

applyUserMode();
syncNativeMenuMode();
listenForCreateMenu();
listenForSettingsMenu();
listenForSaveMenu();
listenForOpenMenu();
listenForExitMenu();
