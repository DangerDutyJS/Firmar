/**
 * Textos legales de Firmar Vecipp. Al cambiar los términos de forma sustancial, sube
 * `TERMS_VERSION`: la app volverá a pedir la aceptación.
 */

export const TERMS_VERSION = "2026-10-29.3";

export const LEGAL_ENTITY = {
  company: "Vecipp Soluciones",
  owners: ["Yilmer Carrillo Diaz", "Yeferson Carrillo"],
  email: "yilmercarrillo74@gmail.com",
  country: "Colombia",
};

export interface LegalSection {
  title: string;
  paragraphs: string[];
}

const { company, email, country } = LEGAL_ENTITY;
const owners = LEGAL_ENTITY.owners.join(" y ");
const effective = "29 de octubre de 2026";

export const TERMS: LegalSection[] = [
  {
    title: "1. Quién ofrece Firmar Vecipp",
    paragraphs: [
      `Firmar Vecipp es un programa de ${company}, cuyos responsables son ${owners} (${country}). Contacto: ${email}.`,
      "Al instalar o usar Firmar Vecipp aceptas estos términos. Si no estás de acuerdo, no uses el programa.",
    ],
  },
  {
    title: "2. Qué tipo de firma produce Firmar Vecipp",
    paragraphs: [
      "Firmar Vecipp produce una firma electrónica simple en los términos de la Ley 527 de 1999 y del Decreto 2364 de 2012 (compilado en el Decreto 1074 de 2015): estampa la imagen de tu firma y añade un sello criptográfico (Ed25519 y SHA-256) que permite detectar si el documento cambió después de firmarlo.",
      "Firmar Vecipp NO es una firma digital certificada: no emite ni utiliza certificados de una entidad de certificación digital acreditada por el ONAC, ni verifica la identidad de quien firma. Por tanto, no se presume su autenticidad como ocurre con la firma digital del artículo 28 de la Ley 527 de 1999.",
      "La firma electrónica simple es admisible como prueba, pero si se discute su autenticidad corresponde demostrarla con otros medios. Tú eres responsable de decidir si este tipo de firma es suficiente para cada documento o trámite. Algunos actos exigen firma digital certificada, firma manuscrita, autenticación notarial u otras formalidades, y Firmar Vecipp no las sustituye.",
    ],
  },
  {
    title: "3. Lo que Firmar Vecipp puede y no puede hacer",
    paragraphs: [
      "Firmar Vecipp detecta cambios en un documento firmado, pero no puede impedir que otra persona o programa lo edite, lo copie o quite la imagen de la firma. La verificación de la integridad se hace abriendo el archivo en Firmar Vecipp.",
      "Si otro programa vuelve a guardar el documento, por ejemplo Word o un editor de PDF, el sello criptográfico se pierde o deja de coincidir, aunque la imagen de la firma siga visible.",
    ],
  },
  {
    title: "4. Tus responsabilidades",
    paragraphs: [
      "El nombre que pones en «Identidad» lo declaras tú y Firmar Vecipp no lo verifica. Un documento solo aparece como «contacto verificado» para quien haya importado la tarjeta de identidad del firmante.",
      "Usa Firmar Vecipp solo con tu propia firma y en documentos que tengas derecho a firmar. Suplantar la firma de otra persona puede constituir delito (por ejemplo, falsedad en documento privado, artículo 289 del Código Penal colombiano).",
      "Tus claves, firmas y copias de los documentos se guardan únicamente en tu equipo. Eres responsable de proteger el acceso a tu computador y de crear y conservar copias de seguridad. Si pierdes el equipo o la contraseña de tu copia de seguridad, nadie, incluido el responsable de Firmar Vecipp, podrá recuperarlas.",
    ],
  },
  {
    title: "5. Licencia de uso",
    paragraphs: [
      `${company} te concede una licencia personal, no exclusiva e intransferible para instalar y usar Firmar Vecipp. No puedes revender el programa ni presentarlo como un servicio de certificación digital. Los componentes de terceros incluidos se rigen por sus propias licencias, que puedes consultar en «Acerca de · Licencias».`,
    ],
  },
  {
    title: "6. Garantía y responsabilidad",
    paragraphs: [
      "Firmar Vecipp se ofrece «tal como está». En la medida en que la ley lo permita, no se garantiza que sea apto para un fin particular ni que esté libre de errores, y el responsable de Firmar Vecipp no responde por daños indirectos, lucro cesante, pérdida de datos ni por el rechazo de un documento por parte de un tercero o una autoridad.",
      "Nada de lo anterior limita los derechos que la Ley 1480 de 2011 (Estatuto del Consumidor) te reconoce como consumidor, incluidos la garantía legal y, en ventas a distancia, el derecho de retracto dentro de los cinco (5) días hábiles siguientes a la compra.",
    ],
  },
  {
    title: "7. Cambios y ley aplicable",
    paragraphs: [
      "Firmar Vecipp puede ofrecerte actualizaciones. Solo se instalan si las aceptas, y antes se verifica que estén firmadas por el responsable.",
      "Si estos términos cambian, Firmar Vecipp te pedirá aceptarlos de nuevo antes de seguir usándolo.",
      `Estos términos se rigen por las leyes de la República de ${country}. Vigentes desde el ${effective}.`,
    ],
  },
];

export const PRIVACY: LegalSection[] = [
  {
    title: "1. Responsable",
    paragraphs: [
      `${company} (responsables: ${owners}), ${country}. Correo para consultas y reclamos: ${email}. Esta política cumple la Ley 1581 de 2012 y el Decreto 1377 de 2013 (compilado en el Decreto 1074 de 2015).`,
    ],
  },
  {
    title: "2. Tus firmas y documentos no salen de tu equipo",
    paragraphs: [
      "Firmar Vecipp firma y verifica documentos sin conexión, no tiene cuentas de usuario, no usa analítica ni telemetría y no envía tus documentos, firmas ni historial al responsable ni a terceros.",
      "La única conexión a internet es la búsqueda de actualizaciones: al abrir la app (o al pulsar «Buscar ahora») se consulta un archivo público en GitHub (GitHub, Inc., Estados Unidos) para saber si hay una versión nueva y, si aceptas instalarla, se descarga desde allí. Como en cualquier conexión, GitHub recibe tu dirección IP y la versión del programa; el responsable de Firmar Vecipp no recibe esos datos. Puedes desactivar la búsqueda automática en «Acerca de».",
      "Todo lo que la app guarda queda en tu equipo, en la carpeta de datos de la aplicación y en el llavero del sistema operativo (Administrador de credenciales de Windows): las imágenes de tus firmas, una copia cifrada de cada documento original que firmas (para poder re-posicionar la firma), el historial de documentos firmados y tus claves criptográficas. Las firmas, los originales y el historial están cifrados con AES-256-GCM.",
    ],
  },
  {
    title: "3. Para qué se usan",
    paragraphs: [
      "Únicamente para que puedas firmar documentos, verificar su integridad, re-posicionar tus firmas y llevar tu propio historial. No se usan con fines comerciales, publicitarios ni de elaboración de perfiles.",
      "Si escribes tu nombre en «Identidad», ese nombre y tu clave pública viajan dentro del sello de cada documento que firmes y en la tarjeta de identidad que decidas compartir, y los verá quien los reciba. Los contactos que importes se guardan cifrados solo en tu equipo.",
      "Si los documentos que firmas contienen datos personales de otras personas, tú eres el responsable de su tratamiento.",
    ],
  },
  {
    title: "4. Copias de seguridad",
    paragraphs: [
      "Cuando creas una copia de seguridad, Firmar Vecipp genera un archivo cifrado con tu contraseña (Argon2id y AES-256-GCM) en el lugar que elijas. Dónde lo guardas (USB, nube…) y quién accede a él lo decides tú.",
    ],
  },
  {
    title: "5. Tus derechos",
    paragraphs: [
      "Como titular puedes conocer, actualizar, rectificar y suprimir tus datos y revocar la autorización (artículo 8 de la Ley 1581 de 2012). Como los datos están solo en tu equipo, puedes ejercer estos derechos directamente desde la app: edita o elimina firmas en «Mis firmas», quita entradas del «Historial» o usa «Copia de seguridad · Borrar todos mis datos».",
      `Para cualquier consulta escribe a ${email}. Las consultas se responden en un máximo de diez (10) días hábiles y los reclamos en quince (15) días hábiles, conforme a los artículos 14 y 15 de la Ley 1581 de 2012. También puedes acudir a la Superintendencia de Industria y Comercio (SIC).`,
    ],
  },
  {
    title: "6. Vigencia",
    paragraphs: [`Esta política está vigente desde el ${effective}. Si cambia, la app te lo indicará.`],
  },
];
