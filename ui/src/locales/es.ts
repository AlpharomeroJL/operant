// Spanish locale catalog. Proof-of-concept translations for wizard and palette.
// All strings must use only user-facing vocabulary from contracts/microcopy_glossary.json.

export const welcomeStrings = {
  heading: "Bienvenido a Operant",
  body: "Operant aprende lo que haces en tu computadora para poder hacerlo por ti.",
  continueButton: "Continuar",
};

export const setupPathStrings = {
  heading: "¿Cómo debe pensar Operant?",
  subheading: "Elige cómo Operant obtiene su poder de pensamiento. Puedes cambiar esto después en configuración.",
  demoLink: "Solo muéstrame una demostración",
  cards: {
    chatgpt: {
      title: "Iniciar sesión con ChatGPT",
      body: "Usa el plan de IA que ya pagas. Sin claves que copiar.",
      button: "Iniciar sesión con ChatGPT",
    },
    claude: {
      title: "Iniciar sesión con Claude",
      body: "Usa el plan de IA que ya pagas. Sin claves que copiar.",
      button: "Iniciar sesión con Claude",
    },
    local: {
      title: "Descarga un cerebro gratuito",
      body: "Obtén un modelo que se ejecuta directamente en esta computadora. Gratis y privado, y continúa funcionando sin internet una vez que se descargue.",
      sizeLabel: (size: string) => `Esta descarga pesa aproximadamente ${size}.`,
      diskCheckLabel: "Comprobando espacio libre en esta computadora.",
      diskCheckOk: "Tienes suficiente espacio para esto.",
      diskCheckLow: (needed: string) => `Esta computadora está baja en espacio. Libera ${needed} e intenta de nuevo.`,
      compatCheckLabel: "Verificando si esta computadora puede ejecutarlo bien.",
      compatCheckOk: "Esta computadora puede ejecutarlo bien.",
      compatCheckSlow: "Esta computadora puede ejecutarlo, pero puede pensar lentamente.",
      compatCheckFail: "Esta computadora no tiene suficiente memoria gráfica para esta opción. Intenta iniciar sesión con ChatGPT o Claude en su lugar.",
      button: "Descargar",
      continueButton: "Continuar",
      download: {
        starting: "Preparándose para descargar.",
        downloading: (percent: number) => `Descargando, ${percent} por ciento completado.`,
        resuming: "Continuando desde donde se dejó.",
        paused: "En pausa. Lo que ya se ha descargado se guarda.",
        pauseButton: "Pausar",
        resumeButton: "Reanudar",
        cancelButton: "Cancelar descarga",
        retryButton: "Intentar de nuevo",
        verifying: "Verificando doble la descarga.",
        complete: "Listo para comenzar.",
        failed: "La descarga no se completó.",
      },
    },
    accessKey: {
      title: "Tengo una clave de acceso",
      body: "Pega la clave de acceso que ya tienes. Operant determinará de dónde es.",
      placeholder: "Pega tu clave de acceso aquí",
      providerLabel: "¿De qué servicio es esta clave?",
      providerAutoDetected: (provider: string) => `Reconocimos esta clave. Parece que es de ${provider}.`,
      providerPickManually: "No pudimos determinar de dónde es esta clave. Elige de la lista a continuación.",
      button: "Continuar",
    },
  },
};

export const providerDisplayNames = {
  chatgpt: "ChatGPT",
  claude: "Claude",
};

export const micCheckStrings = {
  heading: "Verificar tu micrófono",
  body: "Di algo y observa el medidor de nivel moverse. Esto asegura que Operant puede escucharte cuando hablas con él.",
  sampleButton: "Reproducir una muestra",
  levelMeterLabel: "Nivel del micrófono",
  skipButton: "Omitir por ahora",
  continueButton: "Se escucha bien",
  failed: "No pudimos escuchar nada. Verifica que tu micrófono esté conectado e intenta de nuevo.",
};

export const guidedTaskStrings = {
  headingReal: "Intentemos tu primera tarea",
  introReal: "Observa a Operant completar una factura de ejemplo en la página de práctica. Esto enseña a Operant mostrándole qué hacer, paso a paso.",
  headingDemo: "Observando una demostración rápida",
  introDemo: "Aquí está Operant haciendo una tarea sin nada configurado. Nada aquí puede leer, cambiar o enviar nada tuyo.",
  runningLabel: "Trabajando en ello",
  doneLabel: "Hecho. Aquí está todo lo que acaba de hacer.",
  saveButton: "Guardar como flujo de trabajo",
  savedHint: "Guardado. Operant puede hacer esto de nuevo en cualquier momento desde tu lista guardada.",
  demoContinueButton: "Configurarlo de verdad",
  demoContinueHint: "¿Listo para conectarlo a tu propio poder de pensamiento?",
};

export const scheduleStrings = {
  heading: "¿Quieres que se ejecute solo?",
  body: "Elige cuándo Operant debe hacer esto por sí solo, o ejecútalo tú cuando quieras.",
  options: {
    manual: "Solo cuando hago clic en ejecutar",
    daily: "Todos los días",
    weekly: "Cada semana",
    when_file_changes: "Cuando un archivo cambia",
    when_app_opens: "Cuando se abre una aplicación",
    when_email_arrives: "Cuando llega un correo electrónico",
  },
  continueButton: "Guardar este horario",
};

export const downloadErrorStrings = {
  checksum_mismatch: {
    what: "El archivo descargado no coincidía con lo que esperábamos.",
    why: "Esto puede ocurrir si la descarga se interrumpió o la conexión no fue confiable.",
    action: "Intenta descargar de nuevo.",
  },
  network_error: {
    what: "La descarga no pudo llegar al servidor.",
    why: "Esto generalmente significa que la conexión a internet se cortó.",
    action: "Verifica tu conexión e intenta de nuevo.",
  },
  disk_space: {
    what: "No había suficiente espacio para guardar la descarga.",
    why: "Esta computadora tiene poco espacio libre.",
    action: "Libera algo de espacio e intenta de nuevo.",
  },
  not_found: {
    what: "La descarga no pudo encontrar el archivo que buscaba.",
    why: "El enlace a este archivo puede estar desactualizado.",
    action: "Intenta más tarde, o elige una opción diferente.",
  },
};

export const wizardShellStrings = {
  dialogLabel: "Comenzar con Operant",
  stepLabel: (n: number, total: number) => `Paso ${n} de ${total}`,
  progressLabel: "Progreso de la configuración",
};

export const engineStatusStrings = {
  confirm: (name: string) => `Ya está configurado con ${name}.`,
  names: {
    chatgpt: "ChatGPT",
    claude: "Claude",
    local: "el modelo en esta computadora",
  },
  probe: {
    checking: "Comprobando la conexión.",
    reachable: "La conexión funciona bien.",
    unreachable: "No pudimos conectarnos, pero puedes continuar y configurarlo más tarde.",
    not_implemented: "Todavía no pudimos comprobar la conexión.",
    unavailable: "Todavía no pudimos comprobar la conexión.",
  },
};

export const paletteStrings = {
  placeholder: "Dile qué hacer",
  submit: "Enséñalo",
  hint: "Presiona Intro para comenzar a enseñarlo desde lo que está en pantalla ahora",
  overlayLabel: "Paleta de comandos",
  groupWorkflows: "Flujos de trabajo",
  groupActions: "Acciones",
  groupRecent: "Recientes",
  teachThis: "Enseñar esto",
  footerRun: "Intro para ejecutar",
  footerPreview: "Ctrl+Intro para vista previa",
  footerDetails: "Tab para detalles",
  openScreenHint: "Cambiar a esta pantalla",
  themeActionTitle: "Tema",
  settingsHint: "Abrir configuración",
};

export const targetAppStrings = {
  overlayLabel: "Elegir una app para enseñar",
  heading: "Elige la app para enseñar",
  frontApp: "Usar la app que tengo al frente (no Operant)",
  loading: "Buscando tus apps abiertas",
  empty: "No hay otras apps abiertas. Abre la app que quieres enseñar y vuelve a intentarlo.",
  confirmHint: "Intro para enseñar esta app",
  cancelHint: "Esc para volver",
};
