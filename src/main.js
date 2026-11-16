import "./components/math-grid/math-grid.js";

const form = document.querySelector("#workbook-form");
const assignmentList = document.querySelector("#assignment-list");
const status = document.querySelector("#status");
const workbookDescriptionInput = document.querySelector("#workbook-description");
const verificationSection = document.querySelector("#verification-section");
const ratingPicker = document.querySelector("#rating-picker");
const RATING_EMOJIS = Object.freeze(["😟", "😐", "🙂", "😊", "🤩"]);
const settingsDialog = document.querySelector("#settings-dialog");
const aboutDialog = document.querySelector("#about-dialog");
const aboutVersion = document.querySelector("#about-version");
const profileNameLabel = document.querySelector("#profile-name-label");
const profileEmailLabel = document.querySelector("#profile-email-label");
const profileNameInput = document.querySelector("#profile-name");
const profileEmailInput = document.querySelector("#profile-email");
const profileClassField = document.querySelector("#profile-class-field");
const profileClassInput = document.querySelector("#profile-class");
const addAssignmentButton = document.querySelector("#add-assignment");
const senderSection = document.querySelector("#sender-section");
const senderName = document.querySelector("#sender-name");
const toolbarCreate = document.querySelector("#toolbar-create");
const toolbarOpen = document.querySelector("#toolbar-open");
const toolbarSave = document.querySelector("#toolbar-save");
const toolbarSaveAs = document.querySelector("#toolbar-save-as");

const USER_MODES = Object.freeze({
  TEACHER: "teacher",
  STUDENT: "student"
});
const TEACHER_DETAILS_STORAGE_KEY = "pensum-author-details";
const STUDENT_DETAILS_STORAGE_KEY = "pensum-student-details";
const MAX_GRID_ROWS = 200;
const MAX_GRID_COLUMNS = 200;

function showStatus(message, type = "") {
  status.textContent = message;
  status.className = type ? `status status-${type}` : "status";
}

function serializeDocument(documentData) {
  const serializable = { ...documentData };
  delete serializable["created-at"];
  return JSON.stringify(serializable);
}

function getDocumentSnapshot() {
  return serializeDocument(createDocument());
}

function isDirty() {
  return workbookOpen && savedSnapshot !== null && getDocumentSnapshot() !== savedSnapshot;
}

function markWorkbookSaved() {
  savedSnapshot = getDocumentSnapshot();
  syncToolbar();
}

function syncToolbar() {
  if (toolbarSyncTimer !== null) {
    clearTimeout(toolbarSyncTimer);
    toolbarSyncTimer = null;
  }

  const isTeacher = userMode === USER_MODES.TEACHER;
  const canSaveWorkbook = isTeacher || !workbookVerified;
  toolbarCreate.hidden = !isTeacher;
  toolbarCreate.disabled = saveInProgress;
  toolbarOpen.disabled = saveInProgress;
  toolbarSave.hidden = !canSaveWorkbook;
  toolbarSave.disabled =
    !canSaveWorkbook || !workbookOpen || saveInProgress || !isDirty();
  toolbarSaveAs.hidden = !canSaveWorkbook;
  toolbarSaveAs.disabled = !canSaveWorkbook || !workbookOpen || saveInProgress;
}

function scheduleToolbarSync() {
  if (toolbarSyncTimer !== null) clearTimeout(toolbarSyncTimer);
  toolbarSyncTimer = setTimeout(() => {
    toolbarSyncTimer = null;
    syncToolbar();
  }, 200);
}

function loadProfileDetails(storageKey, profileName, includeClass = false) {
  const emptyDetails = includeClass
    ? { name: "", email: "", class: "" }
    : { name: "", email: "" };
  try {
    const savedDetails = localStorage.getItem(storageKey);
    if (!savedDetails) return emptyDetails;

    const details = JSON.parse(savedDetails);
    if (
      !details ||
      typeof details.name !== "string" ||
      typeof details.email !== "string" ||
      (details.class !== undefined && typeof details.class !== "string")
    ) {
      throw new Error(`Պահպանված ${profileName} տվյալները սխալ ձևաչափ ունեն։`);
    }
    return {
      name: details.name,
      email: details.email,
      ...(includeClass ? { class: details.class ?? "" } : {})
    };
  } catch (error) {
    showStatus(`Չհաջողվեց կարդալ ${profileName} կարգավորումները։ ${error.message}`, "error");
    return emptyDetails;
  }
}

let userMode = USER_MODES.STUDENT;
let teacherDetails = loadProfileDetails(TEACHER_DETAILS_STORAGE_KEY, "դասատուի");
let studentDetails = loadProfileDetails(STUDENT_DETAILS_STORAGE_KEY, "սովորողի", true);
let savedSnapshot = null;
let saveInProgress = false;
let currentWorkbookPath = null;
let documentGeneration = 0;
let workbookCreatedAt = null;
let workbookRating = "";
let workbookOpen = false;
let hasLoadedWorkbook = false;
let workbookSolved = false;
let workbookVerified = false;
let workbookTeacher = null;
let workbookStudent = null;
let pendingWorkbookProcessing = Promise.resolve();
let toolbarSyncTimer = null;

function applyUserMode(assignments = collectAssignments()) {
  const isTeacher = userMode === USER_MODES.TEACHER;
  const canEditTeacherContent = isTeacher && !workbookSolved;
  form.hidden = !workbookOpen;
  form.elements.title.readOnly = !canEditTeacherContent;
  workbookDescriptionInput.readOnly = !canEditTeacherContent;
  addAssignmentButton.hidden = !canEditTeacherContent;
  assignmentList.replaceChildren(addAssignmentButton);
  assignments.forEach(createAssignment);
  updateReviewControls();
  updateSenderInfo();
  updateSettingsProfileFields();
  syncToolbar();
}

function updateSenderInfo() {
  const name = workbookStudent?.name?.trim() ?? "";
  const studentClass = workbookStudent?.class?.trim() ?? "";
  const senderText = [name, studentClass].filter(Boolean).join(", ");
  const showSender = userMode === USER_MODES.TEACHER && Boolean(senderText);
  senderSection.hidden = !showSender;
  senderName.textContent = showSender ? senderText : "";
}

function updateReviewControls() {
  const canEditReview = workbookSolved && userMode === USER_MODES.TEACHER;
  verificationSection.hidden = !workbookSolved;
  renderRatingPicker(canEditReview);
}

function renderRatingPicker(enabled) {
  ratingPicker.querySelectorAll("button").forEach(button => {
    button.setAttribute("aria-checked", String(button.dataset.emoji === workbookRating));
    button.disabled = !enabled;
  });
}

ratingPicker.addEventListener("click", event => {
  const button = event.target.closest("button[data-emoji]");
  if (!button || button.disabled) return;
  // Նույն ժպտիկին կրկին սեղմելը ընտրությունը հանում է։
  workbookRating = workbookRating === button.dataset.emoji ? "" : button.dataset.emoji;
  renderRatingPicker(true);
  clearOutput();
});

function updateSettingsProfileFields() {
  const isTeacher = userMode === USER_MODES.TEACHER;
  const details = isTeacher ? teacherDetails : studentDetails;
  profileNameLabel.textContent = "Անուն և ազգանուն";
  profileEmailLabel.textContent = "Էլ․ փոստ";
  profileNameInput.value = details.name;
  profileEmailInput.value = details.email;
  profileClassField.hidden = isTeacher;
  profileClassInput.required = !isTeacher;
  profileClassInput.value = isTeacher ? "" : details.class;
}

function openSettings() {
  updateSettingsProfileFields();
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

  const isTeacher = userMode === USER_MODES.TEACHER;
  const updatedClass = profileClassInput.value.trim();
  const updatedDetails = {
    name: profileNameInput.value.trim(),
    email: profileEmailInput.value.trim(),
    ...(isTeacher ? {} : { class: updatedClass })
  };
  if (
    !updatedDetails.name ||
    !updatedDetails.email ||
    !profileEmailInput.checkValidity() ||
    (!isTeacher && !updatedClass)
  ) {
    showStatus(
      isTeacher
        ? "Լրացրու դասատուի անունն ու վավեր էլ․ փոստը։"
        : "Լրացրու սովորողի անունը, վավեր էլ․ փոստը և դասարանը։",
      "error"
    );
    return;
  }

  const previousEmail = isTeacher ? teacherDetails.email : studentDetails.email;
  const detailsStorageKey = isTeacher ? TEACHER_DETAILS_STORAGE_KEY : STUDENT_DETAILS_STORAGE_KEY;
  try {
    localStorage.setItem(detailsStorageKey, JSON.stringify(updatedDetails));
  } catch (error) {
    showStatus(`Չհաջողվեց պահպանել կարգավորումները։ ${error.message}`, "error");
    return;
  }

  if (isTeacher) {
    teacherDetails = updatedDetails;
  } else {
    studentDetails = updatedDetails;
  }
  applyUserMode();
  if (normalizeEmail(updatedDetails.email) !== normalizeEmail(previousEmail)) {
    documentGeneration += 1;
    currentWorkbookPath = null;
  }
  showStatus("Կարգավորումները պահպանվեցին։");
});

async function listenForSettingsMenu() {
  try {
    await window.__TAURI__.event.listen("open-settings", openSettings);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել կարգավորումների մենյուն։ ${error.message}`, "error");
  }
}

async function importTeacherCertificate() {
  if (userMode !== USER_MODES.STUDENT) {
    showStatus("Դասատուի սերտիֆիկատն արդեն ներմուծված է։", "error");
    return;
  }

  let path;
  try {
    path = await window.__TAURI__.dialog.open({
      title: "Սերտիֆիկատ",
      multiple: false,
      directory: false,
      filters: [{ name: "Դասատուի սերտիֆիկատ", extensions: ["pdcert"] }]
    });
  } catch (error) {
    showStatus(`Չհաջողվեց բացել սերտիֆիկատի ընտրության պատուհանը։ ${error.message}`, "error");
    return;
  }
  if (!path) return;

  if (isDirty()) {
    if (!(await saveWorkbook())) {
      showStatus("Սերտիֆիկատը չներմուծվեց․ նախ պահպանիր բաց տետրը։", "error");
      return;
    }
    if (isDirty()) {
      showStatus(
        "Սերտիֆիկատը չներմուծվեց․ տետրում դեռ չպահպանված փոփոխություններ կան։",
        "error"
      );
      return;
    }
  }

  try {
    const role = await window.__TAURI__.core.invoke("import_teacher_certificate", { path });
    if (role !== USER_MODES.TEACHER) {
      throw new Error("Backend-ը չհաստատեց դասատուի դերը։");
    }
    userMode = USER_MODES.TEACHER;
    closeWorkbook();
    await syncNativeMenuMode();
    openSettings();
    showStatus(
      "Դասատուի սերտիֆիկատը ներմուծվեց։ Լրացրու դասատուի տվյալները կարգավորումներում։",
      "success"
    );
  } catch (error) {
    console.error("Դասատուի սերտիֆիկատի ներմուծումը ձախողվեց։", error);
    showStatus(
      "Սերտիֆիկատը չընդունվեց։ Ստուգիր, որ ընտրել ես ճիշտ .pdcert ֆայլ։",
      "error"
    );
  }
}

async function listenForCertificateImportMenu() {
  try {
    await window.__TAURI__.event.listen("import-teacher-certificate", importTeacherCertificate);
  } catch (error) {
    showStatus(
      `Չհաջողվեց միացնել սերտիֆիկատի ներմուծման մենյուն։ ${error.message}`,
      "error"
    );
  }
}

async function listenForAboutMenu() {
  try {
    await window.__TAURI__.event.listen("open-about", event => {
      if (typeof event.payload !== "string") {
        showStatus("Ծրագրի տարբերակի տվյալները սխալ ձևաչափ ունեն։", "error");
        return;
      }
      aboutVersion.textContent = event.payload;
      if (!aboutDialog.open) aboutDialog.showModal();
    });
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել «Հուշում» մենյուն։ ${error.message}`, "error");
  }
}

async function syncNativeMenuMode() {
  try {
    await window.__TAURI__.core.invoke("set_workbook_verified", {
      verified: workbookVerified
    });
  } catch (error) {
    showStatus(`Չհաջողվեց կիրառել մենյուն։ ${error.message}`, "error");
  }
}

async function checkForUpdates() {
  await new Promise(resolve => setTimeout(resolve, 3000));
  await pendingWorkbookProcessing;

  try {
    const version = await window.__TAURI__.core.invoke("check_for_updates");
    if (!version) return;

    if (isDirty()) {
      showStatus(
        `Հասանելի է ${version} տարբերակը․ նախ պահպանիր տետրը, ապա վերագործարկիր ծրագիրը։`,
        "error"
      );
      return;
    }

    const confirmed = await window.__TAURI__.dialog.confirm(
      `Հասանելի է հավելվածի նոր՝ ${version} տարբերակը։ Ներբեռնե՞լ և տեղադրե՞լ այն հիմա։`,
      { title: "Հավելվածի թարմացում", kind: "info" }
    );
    if (!confirmed) return;

    if (isDirty()) {
      showStatus("Թարմացումից առաջ պահպանիր տետրի փոփոխությունները։", "error");
      return;
    }

    showStatus(`Ներբեռնվում է ${version} թարմացումը…`);
    const unlisten = await window.__TAURI__.event.listen("update-progress", ({ payload }) => {
      if (payload.contentLength) {
        const percent = Math.min(
          100,
          Math.round(payload.downloadedBytes / payload.contentLength * 100)
        );
        showStatus(`Ներբեռնվում է ${version} թարմացումը… ${percent}%`);
      } else {
        const megabytes = (payload.downloadedBytes / 1024 / 1024).toFixed(1);
        showStatus(`Ներբեռնվում է ${version} թարմացումը… ${megabytes} ՄԲ`);
      }
    });
    try {
      await window.__TAURI__.core.invoke("install_update", { expectedVersion: version });
    } finally {
      unlisten();
    }
  } catch (error) {
    showStatus(`Թարմացման գործընթացը չհաջողվեց։ ${error.message}`, "error");
  }
}

async function confirmDiscardChanges(message) {
  if (!isDirty()) return true;
  return window.__TAURI__.dialog.confirm(
    `Չպահպանված փոփոխությունները կկորչեն։ ${message}`,
    { title: "Չպահպանված փոփոխություններ", kind: "warning" }
  );
}

async function createWorkbook() {
  if (userMode !== USER_MODES.TEACHER) return;

  try {
    if (!(await confirmDiscardChanges("Ստեղծե՞լ նոր դատարկ տետր։"))) return;

    documentGeneration += 1;
    workbookOpen = true;
    form.elements.title.value = "";
    workbookDescriptionInput.value = "";
    assignmentList.replaceChildren(addAssignmentButton);
    hasLoadedWorkbook = false;
    workbookSolved = false;
    workbookVerified = false;
    workbookTeacher = null;
    workbookStudent = null;
    workbookRating = "";
    workbookCreatedAt = null;
    currentWorkbookPath = null;
    markWorkbookSaved();
    applyUserMode();
    await syncNativeMenuMode();
    showStatus("Ստեղծվեց նոր դատարկ տետր։", "success");
  } catch (error) {
    showStatus(`Չհաջողվեց ստեղծել նոր տետրը։ ${error.message}`, "error");
  }
}

function closeWorkbook() {
  documentGeneration += 1;
  workbookOpen = false;
  form.elements.title.value = "";
  workbookDescriptionInput.value = "";
  assignmentList.replaceChildren(addAssignmentButton);
  hasLoadedWorkbook = false;
  workbookSolved = false;
  workbookVerified = false;
  workbookTeacher = null;
  workbookStudent = null;
  workbookRating = "";
  workbookCreatedAt = null;
  currentWorkbookPath = null;
  applyUserMode();
  markWorkbookSaved();
}

async function listenForCreateMenu() {
  try {
    await window.__TAURI__.event.listen("create-workbook", createWorkbook);
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել ստեղծման մենյուն։ ${error.message}`, "error");
  }
}

// Լռելյայն ֆայլի անունը՝ «վերնագիր[-սովորող]-լուծված[-գնահատված].pdw»։
// Սովորողի անունը վերցվում է documentData.student-ից, որ լինի և՛ սովորողի, և՛ դասատուի
// ռեժիմում (դասատուի ռեժիմում studentDetails-ը դասատուի սեփական տվյալներն են)։
function buildWorkbookFileName(documentData) {
  const invalidFileNameCharacters = /[<>:"/\\|?*\u0000-\u001f]/g;
  const sanitize = text => text.trim().replace(invalidFileNameCharacters, "-");
  const parts = [sanitize(documentData.title) || "workbook"];
  const studentName = documentData.student?.name?.trim();
  if (documentData.solved && studentName) {
    parts.push(sanitize(studentName).replace(/\s+/g, "-"));
  }
  if (documentData.solved) {
    parts.push("լուծված");
  }
  if (documentData.solved && documentData.verified !== undefined) {
    parts.push("գնահատված");
  }
  return `${parts.join("-")}.pdw`;
}

async function saveWorkbook({ saveAs = false } = {}) {
  if (!workbookOpen) {
    showStatus(
      userMode === USER_MODES.TEACHER ? "Նախ ստեղծիր կամ բացիր տետր։" : "Նախ բացիր տետրը։",
      "error"
    );
    return false;
  }
  if (userMode === USER_MODES.STUDENT && workbookVerified) {
    showStatus("Ստուգված տետրը միայն դիտման համար է։", "error");
    return false;
  }
  if (userMode === USER_MODES.STUDENT && !hasLoadedWorkbook) {
    showStatus("Պահպանելու համար նախ բացիր ուսուցչի տետրը։", "error");
    return false;
  }
  if (saveInProgress) {
    showStatus("Պահպանումն արդեն ընթացքի մեջ է։", "error");
    return false;
  }
  if (!form.reportValidity()) return false;
  if (userMode === USER_MODES.TEACHER) {
    if (!teacherDetails.name.trim() || !teacherDetails.email.trim()) {
      showStatus("Կարգավորումներում լրացրու դասատուի անունն ու էլ․ փոստը։", "error");
      return false;
    }
    const emailValidator = document.createElement("input");
    emailValidator.type = "email";
    emailValidator.value = teacherDetails.email;
    if (!emailValidator.checkValidity()) {
      showStatus("Կարգավորումներում լրացրու դասատուի վավեր էլ․ փոստը։", "error");
      return false;
    }
  } else {
    if (!studentDetails.name.trim() || !studentDetails.email.trim()) {
      showStatus("Կարգավորումներում լրացրու սովորողի անունն ու էլ․ փոստը։", "error");
      return false;
    }
    if (!studentDetails.class?.trim()) {
      showStatus("Կարգավորումներում լրացրու սովորողի դասարանը։", "error");
      return false;
    }
    const emailValidator = document.createElement("input");
    emailValidator.type = "email";
    emailValidator.value = studentDetails.email;
    if (!emailValidator.checkValidity()) {
      showStatus("Կարգավորումներում լրացրու սովորողի վավեր էլ․ փոստը։", "error");
      return false;
    }
  }

  saveInProgress = true;
  syncToolbar();
  try {
    const generationAtStart = documentGeneration;
    const modeAtStart = userMode;
    const documentData = createDocument();
    const snapshotAtStart = serializeDocument(documentData);
    let path = saveAs ? null : currentWorkbookPath;
    if (!path) {
      path = await window.__TAURI__.dialog.save({
        title: "Պահպանել տետրը",
        defaultPath: buildWorkbookFileName(documentData),
        filters: [{ name: "PDW տետր (ZIP)", extensions: ["pdw"] }]
      });
      if (!path) return false;
    }

    await window.__TAURI__.core.invoke("save_workbook", {
      path,
      workbook: documentData
    });
    const documentUnchanged =
      documentGeneration === generationAtStart && userMode === modeAtStart;
    if (documentUnchanged) {
      workbookSolved = documentData.solved;
      workbookVerified = workbookRating !== "";
      workbookCreatedAt = documentData["created-at"];
      currentWorkbookPath = path;
      savedSnapshot = snapshotAtStart;
    }
    await syncNativeMenuMode();
    if (!documentUnchanged) {
      showStatus("Տետրը պահպանվեց, բայց պահպանման ընթացքում ընթացիկ տետրը փոխվեց։", "error");
    } else if (isDirty()) {
      showStatus("Տետրը պահպանվեց, բայց պահպանման ընթացքում նոր փոփոխություններ կան։", "error");
    } else {
      showStatus("Տետրը պահպանվեց PDW ֆայլով։", "success");
    }
    return true;
  } catch (error) {
    showStatus(`Չհաջողվեց պահպանել տետրը։ ${error.message}`, "error");
    return false;
  } finally {
    saveInProgress = false;
    syncToolbar();
  }
}

async function listenForSaveMenu() {
  try {
    await window.__TAURI__.event.listen("save-workbook", () => saveWorkbook());
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել պահպանման մենյուն։ ${error.message}`, "error");
  }
}

async function listenForSaveAsMenu() {
  try {
    await window.__TAURI__.event.listen("save-workbook-as", () => saveWorkbook({ saveAs: true }));
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել «Պահպանել որպես» մենյուն։ ${error.message}`, "error");
  }
}

async function openWorkbookFromPath(path) {
  if (!(await confirmDiscardChanges("Բացե՞լ ընտրված տետրը։"))) return;
  const documentData = await window.__TAURI__.core.invoke("open_workbook", { path });
  loadWorkbookDocument(documentData);
  currentWorkbookPath = isOwnWorkbook(documentData) ? path : null;
  await syncNativeMenuMode();
}

// Ֆայլը կարելի է պահել նույն տեղում միայն, եթե այն ստեղծել է ընթացիկ օգտվողը։
// Ուրիշի ֆայլը (օր․ ուսուցչի տետրը սովորողի համար կամ սովորողի լուծումը ուսուցչի համար)
// միշտ պահվում է որպես նոր ֆայլ, որպեսզի օրիգինալը չվերագրվի։
function normalizeEmail(email) {
  return email.trim().toLowerCase();
}

function isOwnWorkbook(documentData) {
  if (userMode === USER_MODES.TEACHER) {
    return documentData.student === undefined
      && normalizeEmail(documentData.teacher.email) === normalizeEmail(teacherDetails.email);
  }
  return documentData.student !== undefined
    && normalizeEmail(documentData.student.email) === normalizeEmail(studentDetails.email);
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

    await openWorkbookFromPath(path);
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

function processPendingWorkbookFiles() {
  pendingWorkbookProcessing = pendingWorkbookProcessing.then(async () => {
    const paths = await window.__TAURI__.core.invoke("take_pending_workbook_paths");
    for (const path of paths) {
      try {
        await openWorkbookFromPath(path);
      } catch (error) {
        showStatus(`Չհաջողվեց բացել ստացված տետրը։ ${error.message}`, "error");
      }
    }
  }).catch(error => {
    showStatus(`Չհաջողվեց ստանալ բացվող տետրի ուղին։ ${error.message}`, "error");
  });
  return pendingWorkbookProcessing;
}

async function listenForPendingWorkbookFiles() {
  try {
    await window.__TAURI__.event.listen("open-workbook-files", processPendingWorkbookFiles);
    await processPendingWorkbookFiles();
  } catch (error) {
    showStatus(`Չհաջողվեց միացնել ֆայլի բացման հոսքը։ ${error.message}`, "error");
  }
}

async function exitWorkbook() {
  if (saveInProgress) {
    showStatus("Պահպանումն ընթացքի մեջ է․ սպասիր, ապա փակիր ծրագիրը։", "error");
    return;
  }
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
    await window.__TAURI__.event.listen("close-requested", exitWorkbook);
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

function addButtonIcon(button, iconName) {
  const icon = document.createElement("img");
  icon.src = new URL(`./assets/icons/${iconName}.svg`, import.meta.url).href;
  icon.alt = "";
  icon.draggable = false;
  button.append(icon);
}

function updateAssignmentNumbers() {
  assignmentList.querySelectorAll(".assignment-card").forEach((card, index) => {
    card.querySelector(".assignment-number").textContent = `Առաջադրանք ${index + 1}`;
  });
}

function isValidSquaredContent(content) {
  if (!content || typeof content !== "object" || Array.isArray(content)) return false;

  if (
    content.version !== 2 ||
    !Number.isSafeInteger(content.rows) ||
    content.rows < 1 ||
    content.rows > MAX_GRID_ROWS ||
    !Number.isSafeInteger(content.columns) ||
    content.columns < 1 ||
    content.columns > MAX_GRID_COLUMNS ||
    typeof content.cells !== "string" ||
    !Array.isArray(content.borders)
  ) {
    return false;
  }

  const gridRows = content.cells.split("\n");
  return gridRows.length === content.rows &&
    gridRows.every(row => [...row].length === content.columns &&
      [...row].every(character => character === " " || /^[0-9+\-−*/×÷=()]$/u.test(character))) &&
    content.borders.every(border => border &&
      Number.isInteger(border.cell) &&
      border.cell >= 0 &&
      border.cell < content.rows * content.columns &&
      ["bottom", "left"].includes(border.edge));
}

function createSquaredEditor(initialContent, editable = true) {
  const scroll = document.createElement("div");
  scroll.className = "grid-scroll";
  const grid = document.createElement("math-grid");
  grid.setAttribute("aria-label", "Վանդակավոր դաշտ");
  grid.toggleAttribute("disabled", !editable);
  if (initialContent && typeof initialContent === "object" && !Array.isArray(initialContent)) {
    grid.value = JSON.stringify(initialContent);
  }
  grid.addEventListener("input", () => clearOutput());
  scroll.append(grid);
  return scroll;
}

function createContentBlock(type, imageUrl, initialContent = "", editable = true) {
  const block = document.createElement("div");
  block.className = "content-block";
  block.dataset.type = type;

  const removeButton = makeButton("", "button button-danger remove-block-button", () => {
    block.remove();
    clearOutput();
  });
  addButtonIcon(removeButton, "x");
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
  input.accept = "image/png,image/jpeg";
  input.hidden = true;
  input.setAttribute("aria-label", "Ընտրել պատկեր");
  document.body.append(input);

  input.addEventListener("cancel", () => input.remove(), { once: true });
  input.addEventListener("change", () => {
    const file = input.files[0];
    input.remove();
    if (!file) return;
    if (!["image/png", "image/jpeg"].includes(file.type)) {
      status.textContent = "Ընտրիր PNG կամ JPEG պատկեր։";
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
      blocks.append(createContentBlock("image", reader.result));
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

  const heading = title ? document.createElement("div") : null;
  if (heading) {
    heading.className = "section-heading";
    const titleElement = document.createElement("h4");
    titleElement.textContent = title;
    heading.append(titleElement);
  }

  const blocks = document.createElement("div");
  blocks.className = "block-list";
  initialBlocks.forEach(savedBlock => {
    let block;
    if (savedBlock.type === "text") {
      block = createContentBlock("text", undefined, savedBlock.content, editable);
    } else if (savedBlock.type === "image") {
      block = createContentBlock("image", savedBlock.url, "", editable);
    } else {
      block = createContentBlock("squared", undefined, savedBlock.content, editable);
    }
    blocks.append(block);
  });
  const tools = document.createElement("div");
  tools.className = "add-block-tools";
  tools.append(
    makeButton("", "add-block-button add-block-button-text", () => {
      blocks.append(createContentBlock("text"));
      clearOutput();
    }),
    makeButton("", "add-block-button add-block-button-image", () => openImagePicker(blocks)),
    makeButton("", "add-block-button add-block-button-grid", () => {
      blocks.append(createContentBlock("squared"));
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
  if (heading) section.append(heading);
  section.append(blocks, tools);
  return section;
}

function createTeacherComment(value, readOnly, assignmentNumber, onRemove) {
  const field = document.createElement("div");
  field.className = "teacher-comment field";
  const heading = document.createElement("div");
  heading.className = "teacher-comment-heading";
  const label = document.createElement("label");
  label.htmlFor = `teacher-comment-${assignmentNumber}`;
  label.textContent = "Մեկնաբանություն";
  const textarea = document.createElement("textarea");
  textarea.id = label.htmlFor;
  textarea.setAttribute("aria-label", `Առաջադրանք ${assignmentNumber}-ի մեկնաբանություն`);
  textarea.value = value;
  textarea.readOnly = readOnly;
  heading.append(label);
  if (!readOnly) {
    const removeButton = makeButton("", "button button-danger remove-block-button", onRemove);
    addButtonIcon(removeButton, "x");
    removeButton.setAttribute("aria-label", "Հեռացնել մեկնաբանությունը");
    removeButton.title = "Հեռացնել մեկնաբանությունը";
    heading.append(removeButton);
  }
  field.append(heading, textarea);
  return field;
}

function createAddCommentButton(assignmentNumber, sections) {
  const button = makeButton("", "button button-secondary add-comment-button");
  const icon = document.createElement("img");
  icon.src = new URL("./assets/icons/message-square-plus.svg", import.meta.url).href;
  icon.alt = "";
  icon.draggable = false;
  button.append(icon);
  button.setAttribute(
    "aria-label",
    `Ավելացնել մեկնաբանություն առաջադրանք ${assignmentNumber}-ին`
  );
  button.title = "Մեկնաբանություն";
  button.addEventListener("click", () => {
    const commentField = createTeacherComment("", false, assignmentNumber, () => {
      commentField.replaceWith(createAddCommentButton(assignmentNumber, sections));
      clearOutput();
    });
    button.replaceWith(commentField);
    commentField.querySelector("textarea").focus();
    clearOutput();
  });
  return button;
}

function createAssignment(savedAssignment = { problem: [], solution: [] }) {
  const card = document.createElement("article");
  card.className = "assignment-card";
  const isTeacher = userMode === USER_MODES.TEACHER;
  const canEditTeacherContent = isTeacher && !workbookSolved;
  const canEditStudentSolution = !isTeacher && !workbookVerified;
  const savedSolution = savedAssignment.solution;

  const heading = document.createElement("div");
  heading.className = "assignment-heading";
  const title = document.createElement("h4");
  title.className = "assignment-number";
  const removeButton = makeButton("", "button button-danger assignment-action-button remove-block-button", () => {
    card.remove();
    updateAssignmentNumbers();
  });
  addButtonIcon(removeButton, "x");
  removeButton.hidden = !canEditTeacherContent;
  removeButton.setAttribute("aria-label", "Հեռացնել առաջադրանքը");
  removeButton.title = "Հեռացնել առաջադրանքը";
  heading.append(
    title,
    removeButton
  );
  removeButton.addEventListener("click", () => clearOutput());

  const sections = document.createElement("div");
  sections.className = "assignment-sections";
  sections.append(
    createContentSection("problem", null, savedAssignment.problem, canEditTeacherContent),
  );
  if (!isTeacher || savedSolution.length > 0 || workbookSolved) {
    sections.append(
      createContentSection("solution", "Լուծում", savedSolution, canEditStudentSolution)
    );
  }
  if (workbookSolved) {
    const assignmentNumber = assignmentList.querySelectorAll(".assignment-card").length + 1;
    const savedComment = savedAssignment["teacher-comment"] ?? "";
    if (savedComment.trim()) {
      const commentField = createTeacherComment(
        savedComment,
        !isTeacher,
        assignmentNumber,
        () => {
          commentField.replaceWith(createAddCommentButton(assignmentNumber, sections));
          clearOutput();
        }
      );
      sections.append(commentField);
    } else if (isTeacher) {
      sections.append(createAddCommentButton(assignmentNumber, sections));
    }
  }
  card.append(heading, sections);
  assignmentList.insertBefore(card, addAssignmentButton);
  updateAssignmentNumbers();
}

function clearOutput(message = "Փոփոխություններ կան․ տետրը պահպանելու համար ընտրիր «Պահպանել»։") {
  scheduleToolbarSync();
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
    const teacherComment = card.querySelector(".teacher-comment textarea");
    return {
      problem: collectBlocks(problemSection),
      solution: solutionSection
        ? collectBlocks(solutionSection)
        : [],
      ...(teacherComment?.value.trim()
        ? { "teacher-comment": teacherComment.value }
        : {})
    };
  });
}

function createDocument() {
  return {
    title: form.elements.title.value,
    description: workbookDescriptionInput.value,
    teacher: userMode === USER_MODES.STUDENT ? workbookTeacher : { ...teacherDetails },
    ...(userMode === USER_MODES.STUDENT
      ? { student: { ...studentDetails } }
      : workbookStudent
        ? { student: { ...workbookStudent } }
        : {}),
    "created-at": workbookCreatedAt ?? getLocalDateString(),
    solved: userMode === USER_MODES.STUDENT || workbookSolved,
    assignments: collectAssignments(),
    ...(workbookRating === "" ? {} : { verified: { emoji: workbookRating } })
  };
}

function validateWorkbookDocument(documentData) {
  if (!documentData || typeof documentData !== "object" || Array.isArray(documentData)) {
    throw new Error("Տետրի տվյալները սխալ ձևաչափ ունեն։");
  }
  if (documentData.formatVersion !== 2) {
    throw new Error("Տետրի ZIP ձևաչափի տարբերակը չի աջակցվում։");
  }
  if (
    typeof documentData.title !== "string" ||
    typeof documentData.description !== "string" ||
    !(
      documentData.teacher &&
      typeof documentData.teacher === "object" &&
      typeof documentData.teacher.name === "string" &&
      typeof documentData.teacher.email === "string"
    ) ||
    (documentData.student !== undefined && (
      !documentData.student ||
      typeof documentData.student !== "object" ||
      typeof documentData.student.name !== "string" ||
      typeof documentData.student.email !== "string" ||
      (documentData.student.class !== undefined && typeof documentData.student.class !== "string")
    )) ||
    typeof documentData.solved !== "boolean" ||
    (documentData.verified !== undefined && (
      !documentData.verified ||
      typeof documentData.verified !== "object" ||
      Array.isArray(documentData.verified) ||
      !RATING_EMOJIS.includes(documentData.verified.emoji)
    )) ||
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
      !Array.isArray(assignment.solution) ||
      (assignment["teacher-comment"] !== undefined &&
        typeof assignment["teacher-comment"] !== "string")
    ) {
      throw new Error("Առաջադրանքի տվյալները սխալ ձևաչափ ունեն։");
    }
    for (const blocks of [assignment.problem, assignment.solution]) {
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
  documentGeneration += 1;
  workbookOpen = true;
  form.elements.title.value = documentData.title;
  workbookDescriptionInput.value = documentData.description;
  workbookTeacher = documentData.teacher;
  workbookStudent = documentData.student ?? null;
  hasLoadedWorkbook = true;
  workbookSolved = documentData.solved;
  workbookVerified = documentData.verified !== undefined;
  workbookCreatedAt = documentData["created-at"];
  workbookRating = documentData.verified?.emoji ?? "";
  applyUserMode(documentData.assignments);
  assignmentList.querySelectorAll(".content-block-text textarea").forEach(textarea => {
    textarea.style.height = "auto";
    const styles = getComputedStyle(textarea);
    const borderHeight = parseFloat(styles.borderTopWidth) + parseFloat(styles.borderBottomWidth);
    textarea.style.height = `${textarea.scrollHeight + borderHeight}px`;
  });
  markWorkbookSaved();
  showStatus(
    workbookSolved && userMode === USER_MODES.TEACHER
      ? "Լուծված տետրը բացվեց․ պայմաններն ու լուծումները read-only են, կարող ես ավելացնել մեկնաբանություններ և գնահատական։"
      : workbookSolved
        ? "Գրախոսված տետրը բացվեց․ գնահատականն ու մեկնաբանությունները դիտման համար են։"
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

toolbarCreate.addEventListener("click", createWorkbook);
toolbarOpen.addEventListener("click", openWorkbook);
toolbarSave.addEventListener("click", () => saveWorkbook());
toolbarSaveAs.addEventListener("click", () => saveWorkbook({ saveAs: true }));

async function initializeApp() {
  try {
    const role = await window.__TAURI__.core.invoke("get_app_role");
    userMode = role === USER_MODES.TEACHER ? USER_MODES.TEACHER : USER_MODES.STUDENT;
  } catch (error) {
    console.error("Չհաջողվեց որոշել օգտվողի ռեժիմը։", error);
    userMode = USER_MODES.STUDENT;
    showStatus(
      "Չհաջողվեց որոշել օգտվողի ռեժիմը։ Ծրագիրը կաշխատի սովորողի ռեժիմով։",
      "error"
    );
  }

  applyUserMode();
  markWorkbookSaved();
  await syncNativeMenuMode();
  await Promise.all([
    listenForCreateMenu(),
    listenForSettingsMenu(),
    listenForCertificateImportMenu(),
    listenForAboutMenu(),
    listenForSaveMenu(),
    listenForSaveAsMenu(),
    listenForOpenMenu(),
    listenForExitMenu()
  ]);
  await listenForPendingWorkbookFiles();
  checkForUpdates();
}

initializeApp();
