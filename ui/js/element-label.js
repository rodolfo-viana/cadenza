import { t, N_ } from './i18n.js';

export const KIND_LABEL = {
  text: N_('Text'),
  shape: N_('Shape'),
  image: N_('Image'),
  video: N_('Video'),
  code: N_('Code'),
  math: N_('Equation'),
  embed: N_('Web Page'),
  table: N_('Table'),
  chart: N_('Chart'),
  diagram: N_('Diagram'),
  camera: N_('Live Camera'),
};

/** An element's display name: its own name, or else the label of its kind (translated). */
export const elementName = (el) => (el.name ? t(el.name) : KIND_LABEL[el.content.kind] && t(KIND_LABEL[el.content.kind]));
