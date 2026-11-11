const templateType = document.querySelector("#template-type");
const addTaskButton = document.querySelector("#add-task");
const taskList = document.querySelector("#task-list");
const emptyState = document.querySelector("#empty-state");

let nextTaskId = 1;

addTaskButton.addEventListener("click", () => {
  const type = templateType.value;
  if (type !== "text" && type !== "arithmetic") {
    throw new TypeError(`Չաջակցվող առաջադրանքի տեսակ՝ ${type}`);
  }

  const card = createTaskCard(type, nextTaskId);
  nextTaskId += 1;
  taskList.append(card);
  updateTaskList();
  card.querySelector("textarea, math-grid input")?.focus();
});

function createTaskCard(type, id) {
  const card = document.createElement("article");
  card.className = "task-card";
  card.dataset.taskId = String(id);
  card.dataset.taskType = type;

  const title = type === "text" ? "Տեքստային առաջադրանք" : "Թվաբանական առաջադրանք";
  card.innerHTML = `
    <header class="task-header">
      <h2 tabindex="-1"></h2>
      <div class="task-actions" aria-label="Առաջադրանքի կառավարում">
        <button class="move-up" type="button" aria-label="Տեղափոխել վերև" title="Տեղափոխել վերև">↑</button>
        <button class="move-down" type="button" aria-label="Տեղափոխել ներքև" title="Տեղափոխել ներքև">↓</button>
        <button class="delete-task" type="button" aria-label="Ջնջել առաջադրանքը" title="Ջնջել առաջադրանքը">Ջնջել</button>
      </div>
    </header>
    <div class="task-fields"></div>
  `;
  card.querySelector("h2").textContent = `${title} ${id}`;

  const fields = card.querySelector(".task-fields");
  if (type === "text") {
    fields.append(createTextTaskFields(id));
  } else {
    fields.append(createArithmeticTaskFields(id));
  }

  card.querySelector(".move-up").addEventListener("click", () => moveTask(card, -1));
  card.querySelector(".move-down").addEventListener("click", () => moveTask(card, 1));
  card.querySelector(".delete-task").addEventListener("click", () => {
    const nextCard = card.nextElementSibling ?? card.previousElementSibling;
    revokeTaskImage(card.querySelector(".image-field"));
    card.remove();
    updateTaskList();
    nextCard?.querySelector("h2").focus();
  });
  return card;
}

function createTextTaskFields(id) {
  const fields = document.createDocumentFragment();

  const promptLabel = document.createElement("label");
  promptLabel.htmlFor = `task-${id}-prompt`;
  promptLabel.textContent = "Առաջադրանքի տեքստ";

  const prompt = document.createElement("textarea");
  prompt.id = `task-${id}-prompt`;
  prompt.name = `task-${id}-prompt`;
  prompt.required = true;
  prompt.placeholder = "Գրեք առաջադրանքի պայմանը…";
  promptLabel.append(prompt);
  fields.append(promptLabel);

  const imageField = document.createElement("div");
  imageField.className = "image-field";
  const imageLabel = document.createElement("label");
  imageLabel.htmlFor = `task-${id}-image`;
  imageLabel.textContent = "Նկար (ըստ ցանկության)";

  const imageInput = document.createElement("input");
  imageInput.type = "file";
  imageInput.id = `task-${id}-image`;
  imageInput.name = `task-${id}-image`;
  imageInput.accept = "image/*";

  const imagePreview = document.createElement("img");
  imagePreview.className = "image-preview";
  imagePreview.alt = `Առաջադրանք ${id}-ի նկարի նախադիտում`;
  imagePreview.hidden = true;

  const removeImage = document.createElement("button");
  removeImage.type = "button";
  removeImage.textContent = "Հեռացնել նկարը";
  removeImage.hidden = true;

  imageInput.addEventListener("change", () => {
    revokeTaskImage(imageField);
    const [file] = imageInput.files;
    if (!file) {
      imagePreview.removeAttribute("src");
      imagePreview.hidden = true;
      removeImage.hidden = true;
      return;
    }
    if (!file.type.startsWith("image/")) {
      imageInput.value = "";
      imagePreview.removeAttribute("src");
      imagePreview.hidden = true;
      removeImage.hidden = true;
      imageInput.setCustomValidity("Ընտրեք պատկերի ֆայլ։");
      imageInput.reportValidity();
      return;
    }

    imageInput.setCustomValidity("");
    imageField.dataset.previewUrl = URL.createObjectURL(file);
    imagePreview.src = imageField.dataset.previewUrl;
    imagePreview.hidden = false;
    removeImage.hidden = false;
  });
  removeImage.addEventListener("click", () => {
    revokeTaskImage(imageField);
    imageInput.value = "";
    imageInput.setCustomValidity("");
    imagePreview.removeAttribute("src");
    imagePreview.hidden = true;
    removeImage.hidden = true;
  });

  imageField.append(imageLabel, imageInput, imagePreview, removeImage);
  fields.append(imageField);
  return fields;
}

function createArithmeticTaskFields(id) {
  const fields = document.createDocumentFragment();

  const promptLabel = document.createElement("label");
  promptLabel.htmlFor = `task-${id}-prompt`;
  promptLabel.textContent = "Առաջադրանքի հրահանգ (ըստ ցանկության)";

  const prompt = document.createElement("textarea");
  prompt.id = `task-${id}-prompt`;
  prompt.name = `task-${id}-prompt`;
  prompt.placeholder = "Օրինակ՝ Հաշվեք արտահայտության արժեքը։";
  promptLabel.append(prompt);

  const gridLabel = document.createElement("p");
  gridLabel.textContent = "Առաջադրանքը կազմեք վանդակներում։";

  const gridScroll = document.createElement("div");
  gridScroll.className = "grid-scroll";
  const grid = document.createElement("math-grid");
  grid.setAttribute("name", `task-${id}-grid`);
  grid.setAttribute("aria-label", `Թվաբանական առաջադրանք ${id}`);
  gridScroll.append(grid);

  fields.append(promptLabel, gridLabel, gridScroll);
  return fields;
}

function moveTask(card, offset) {
  const cards = [...taskList.children];
  const index = cards.indexOf(card);
  const target = index + offset;
  if (target < 0 || target >= cards.length) {
    return;
  }

  if (offset < 0) {
    taskList.insertBefore(card, cards[target]);
  } else {
    taskList.insertBefore(cards[target], card);
  }
  updateTaskList();
  card.querySelector("h2").focus();
}

function updateTaskList() {
  const cards = [...taskList.children];
  emptyState.hidden = cards.length > 0;
  cards.forEach((card, index) => {
    card.querySelector(".move-up").disabled = index === 0;
    card.querySelector(".move-down").disabled = index === cards.length - 1;
  });
}

function revokeTaskImage(element) {
  if (!element) {
    return;
  }
  const previewUrl = element.dataset.previewUrl;
  if (previewUrl) {
    URL.revokeObjectURL(previewUrl);
    delete element.dataset.previewUrl;
  }
}
