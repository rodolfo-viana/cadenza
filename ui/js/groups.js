// Geometry shared by group selection and live canvas previews.
export function expandGroups(slide, ids) {
  if (!slide) return ids;
  const groups = new Set(slide.elements.filter((e) => ids.includes(e.id) && e.group).map((e) => e.group.id));
  return slide.elements.filter((e) => ids.includes(e.id) || groups.has(e.group?.id)).map((e) => e.id);
}

export function selectedGroup(elements) {
  const group = elements[0]?.group;
  return elements.length > 1 && group && elements.every((e) => e.group?.id === group.id) ? group : null;
}

export function transformMember(el, before, after) {
  const k = after.w / before.w,
    a = ((after.rotation - before.rotation) * Math.PI) / 180;
  const dx = (el.x + el.w / 2 - before.x - before.w / 2) * k;
  const dy = (el.y + el.h / 2 - before.y - before.h / 2) * k;
  const w = el.w * k,
    h = el.h * k;
  return {
    x: after.x + after.w / 2 + dx * Math.cos(a) - dy * Math.sin(a) - w / 2,
    y: after.y + after.h / 2 + dx * Math.sin(a) + dy * Math.cos(a) - h / 2,
    w,
    h,
    rotation: el.rotation + after.rotation - before.rotation,
    contentScale: (el.contentScale || 1) * k,
  };
}

export async function editGroup(ungroup = false) {
  const S = await import('./store.js');
  const { endAllEditing } = await import('./textedit.js');
  const docId = S.state.doc?.docId,
    slide = S.state.slideId,
    ids = [...S.state.selection];
  if (!(await endAllEditing()) || S.state.doc?.docId !== docId) return;
  await S.apply({ op: ungroup ? 'ungroupElements' : 'groupElements', slide, ids }, { docId });
}
