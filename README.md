# Pensum Domesticum

**Pensum Domesticum**-ը տնային աշխատանքների տետրեր կազմելու, լրացնելու և ստուգելու desktop հավելված է։ Այն կառուցված է Tauri 2-ի, Rust-ի և վանիլային HTML/CSS/JavaScript-ի հիման վրա։

## Հնարավորություններ

- Դասատուի և սովորողի առանձին աշխատանքային ռեժիմներ։
- Տետրերի ստեղծում, բացում և պահպանում `.pdw` ֆայլերով։
- Թեմայի, նկարագրության և առաջադրանքների կառավարում։
- Տեքստի, պատկերների և վանդակավոր դաշտերի ավելացում։
- Սովորողի լուծումների դիտում, առաջադրանքների մեկնաբանում և 1–20 գնահատականով գրախոսում։
- Ստուգված տետրերի դիտում սովորողի կողմից՝ միայն read-only ռեժիմում։
- Windows-ի տեղադրված տարբերակի թարմացումների ստուգում և օգտվողի հաստատմամբ տեղադրում։

## Տեխնոլոգիաներ

- **Frontend:** HTML, CSS, JavaScript՝ առանց frontend framework-ի կամ package manager-ի։
- **Backend:** Rust՝ Tauri 2-ի միջոցով։
- **Տետրերի ձևաչափ:** `.pdw` ZIP արխիվ՝ `workbook.json` տվյալներով և ներկառուցված պատկերներով։

## Նախագծի կառուցվածք

```text
src/                         Frontend-ի HTML, CSS, JavaScript և պատկերակներ
src/components/math-grid/    Վանդակավոր դաշտի բաղադրիչ
src-tauri/                   Tauri կարգավորումներ և Rust backend
workbook.schema.json         Պահպանվող տետրի տվյալների JSON Schema
```

## Մշակման միջավայր

Պահանջվում են Rust toolchain-ը և Tauri 2-ի համակարգային build prerequisites-ը։ Tauri-ի պահանջներին համապատասխան նախապատրաստեք ձեր օպերացիոն համակարգը՝ [Tauri-ի prerequisites ուղեցույցով](https://v2.tauri.app/start/prerequisites/)։

Տեղադրեք Tauri CLI-ն՝

```sh
cargo install tauri-cli --version "^2.0.0" --locked
```

Մշակման ռեժիմով գործարկելու համար՝

```sh
cd src-tauri
cargo tauri dev
```

Desktop հավելվածը կառուցելու համար՝

```sh
cd src-tauri
cargo tauri build
```

## Ստուգումներ

Հրամանները գործարկեք նախագծի արմատից՝

```sh
node --check src/main.js
node --input-type=module --check < src/components/math-grid/math-grid.js
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Ձեռքով ստուգման դեպքերը և արդյունքների գրանցման ձևը՝ [TEST_PLAN.md](./TEST_PLAN.md)-ում։

## Տվյալների ձևաչափ

`.pdw` ֆայլերը ZIP արխիվներ են․ դրանցում տետրի տվյալները պահվում են `workbook.json`-ում, իսկ պատկերները՝ առանձին ֆայլերով։ Տվյալների կառուցվածքը նկարագրված է [workbook.schema.json](./workbook.schema.json)-ում։

Windows-ում ծրագիրը տեղադրելուց հետո `.pdw` ֆայլին կրկնակի սեղմելով այն կբացվի հավելվածում։ Եթե հավելվածն արդեն բաց է, ընտրված տետրը կբացվի գործող պատուհանում։

## Սահմանափակումներ

- Փոփոխություններն ինքնաշխատ չեն պահպանվում․ դրանք պահելու համար օգտագործեք «Տետր → Պահպանել»։
- Չպահպանված փոփոխություններով տետր փակելիս կամ փոխելիս հավելվածը հաստատում է խնդրում։
