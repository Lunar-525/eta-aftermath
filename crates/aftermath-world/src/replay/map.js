"use strict";

const byId = (id) => document.getElementById(id);
const element = (tag, text) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  return node;
};

const frameKeys = [
  "day", "weather", "population", "food", "water", "fuel", "wood", "steel",
  "pump", "foodShortfall", "waterShortfall", "accepted", "completed", "unfilled",
  "medicine", "medicalCapacity", "treated", "waitingForCare", "kitsPrepared", "cloth", "antiseptic",
  "gridRenewable", "gridBackup", "gridRequested", "gridDelivered", "gridRated",
  "granaryCondition", "granaryCapacity", "waterRequested",
  "waterQuality", "gridVoltage", "gridFrequency",
];
const frames = byId("frames").textContent.trim().split("\n").map((line) =>
  Object.fromEntries(line.split(",").map((value, index) => [
    frameKeys[index], index === 1 ? value : Number(value),
  ])),
);
const foodQuality = byId("food-quality").textContent.trim().split("\n").map((line) => {
  const counts = line.split(",").map(Number);
  return [counts.slice(0, 5), counts.slice(5)];
});
const units = Object.fromEntries(byId("units").textContent.trim().split("\n")
  .map((line) => {
    const [account, scale, unit] = line.split(",");
    return [account, { scale: Number(scale), unit }];
  }));
const photos = new Map(byId("photos").textContent.trim().split("\n").filter(Boolean).map((line) => {
  const separator = line.indexOf(":");
  return [line.slice(0, separator), `data:image/jpeg;base64,${line.slice(separator + 1)}`];
}));
const ordinaryNumber = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 1 });
const tonnes = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 3 });

function measure(account, quantity) {
  const { scale, unit } = units[account];
  const value = quantity * scale / 10;
  if (unit === "kg" && value >= 1_000) return `${tonnes.format(value / 1_000)} t`;
  return `${ordinaryNumber.format(value)} ${unit}`;
}

const events = new Map();
const eventsByDay = new Map();
for (const line of byId("events").textContent.trim().split("\n").slice(1)) {
  const [id, day, cause, subject, reason, change, account, before, after, time, residentId, relatedResidentId] = line.split(",");
  const event = { id: +id, day: +day, cause: +cause, subject, reason, change, account, before, after, time, residentId, relatedResidentId };
  events.set(event.id, event);
  if (!eventsByDay.has(event.day)) eventsByDay.set(event.day, []);
  eventsByDay.get(event.day).push(event);
}

const plotLocations = new Map();
const plotInventory = new Map();
for (const [index, line] of byId("plots").textContent.trim().split("\n").entries()) {
  const [col, row, wood, steel, capacity, accessible] = line.split(",").map(Number);
  const location = `${col},${row}`;
  plotLocations.set(`plot-${index}`, location);
  plotInventory.set(location, { wood, steel, capacity, accessible: Boolean(accessible) });
}
const plotsByDay = frames.map((frame) => {
  for (const event of eventsByDay.get(frame.day) ?? []) {
    if (event.account !== "field_wood" && event.account !== "field_steel") continue;
    const location = plotLocations.get(event.subject);
    const plot = plotInventory.get(location);
    const material = event.account.slice(6);
    const balance = plot[material] + Number(event.after) - Number(event.before);
    plotInventory.set(location, { ...plot, [material]: balance });
    event.plotBalance = balance;
  }
  return new Map(plotInventory);
});

const organizations = new Map();
const residentNames = new Map(byId("residents").textContent.trim().split("\n")
  .map((line) => line.split(",")));
const relationshipSnapshots = new Map([[0, new Map()]]);
const relationshipDeltas = new Map();
const pairKey = (left, right) => `${Math.min(left, right)},${Math.max(left, right)}`;
let checkpoint;
for (const line of byId("relationship-snapshots").textContent.trim().split("\n").filter(Boolean)) {
  const [left, right, count, lastDay] = line.split(",").map(Number);
  if (right === undefined) {
    checkpoint = new Map();
    relationshipSnapshots.set(left, checkpoint);
  } else {
    checkpoint.set(pairKey(left, right), { left, right, count, lastDay });
  }
}
for (const line of byId("relationship-deltas").textContent.trim().split("\n").filter(Boolean)) {
  const [day, ...crew] = line.split(",").map(Number);
  if (!relationshipDeltas.has(day)) relationshipDeltas.set(day, []);
  relationshipDeltas.get(day).push(crew);
}
let relationshipDay;
let relationshipView;
function relationshipsAt(day) {
  if (day === relationshipDay) return relationshipView;
  const base = day - day % 30;
  const pairs = new Map(relationshipSnapshots.get(base));
  for (let date = base + 1; date <= day; date++) {
    for (const crew of relationshipDeltas.get(date) ?? []) {
      for (let i = 0; i < crew.length; i++) {
        for (let j = i + 1; j < crew.length; j++) {
          const left = Math.min(crew[i], crew[j]);
          const right = Math.max(crew[i], crew[j]);
          const key = pairKey(left, right);
          pairs.set(key, { left, right, count: (pairs.get(key)?.count ?? 0) + 1, lastDay: date });
        }
      }
    }
  }
  relationshipDay = day;
  relationshipView = pairs;
  return pairs;
}
for (const line of byId("organizations").textContent.trim().split("\n").filter(Boolean)) {
  const [group, id] = line.split(",");
  if (!organizations.has(group)) organizations.set(group, []);
  organizations.get(group).push(id);
}

const voteResults = [...events.values()].filter((event) => event.change === "vote_tally");
const deaths = new Map([...events.values()].filter((event) => event.reason === "resident_died")
  .map((event) => [event.residentId, event.day]));

const reasonNames = {
  fishing_priority_effective: "渔点优先排班生效",
  food_crew_allocated: "分配农田与渔点班次",
  food_vote_stock_review: "核对粮仓库存",
  food_vote_harvest_review: "复核当日农渔单产",
  fishing_priority_proposed: "提议次日缺工时优先排渔点",
  food_vote_yes: "赞成渔点优先",
  food_vote_no: "反对渔点优先",
  food_vote_passed: "渔点优先提案通过",
  food_vote_rejected: "渔点优先提案否决",
  food_vote_no_quorum: "农渔表决人数不足",
  shift_refused_risk: "因设备风险拒班",
  exchange_signed_stock: "签认物资账",
  backup_stock_review: "核对备用燃料与储水",
  backup_service_review: "评估泵况与水质",
  backup_limit_proposed: "提议次日暂停备用发电",
  backup_vote_yes: "赞成暂缓发电",
  backup_vote_no: "反对暂缓发电",
  backup_vote_passed: "限用提案通过",
  backup_vote_rejected: "限用提案否决",
  backup_vote_no_quorum: "表决人数不足",
  backup_reserve_override: "储水触及应急线，恢复备用发电",
  backup_limit_effective: "备用发电限用生效",
  backup_blocked_by_vote: "按表决结果暂停备用发电",
  coworkers_met: "首次同班完工",
  crew_familiarity_priority: "近期合作改变入选人手",
  clear: "晴天", rain: "阴雨", water_shifts: "水务排班", food_shifts: "农渔排班",
  repair_shifts: "维修排班", salvage_shifts: "回收排班", no_shifts_required: "无需排班",
  cloth_shifts: "织布排班", cloth_food_used: "领用稻草原料", cloth_production: "清洁布料入库",
  antiseptic_shifts: "消毒用品排班", antiseptic_load: "消毒设备用电",
  antiseptic_power_denied: "消毒设备缺电", antiseptic_water_used: "领用备制用水",
  antiseptic_production: "消毒用品入库",
  shift_accepted: "居民接班", shift_refused_health: "因健康拒班",
  shift_refused_fatigue: "因疲劳拒班", shifts_unfilled: "岗位缺员", shift_completed: "完工",
  weather_generation: "按天气核定发电量", archive_load: "档案终端用电", kitchen_load: "公共灶用电",
  backup_fuel: "消耗备用燃料", fuel_shifts: "燃料排班", biomass_fuel_produced: "干生物质入库",
  backup_generation: "应急发电", power_expired: "当日余电清零", water_load: "净水用电",
  repair_load: "维修用电", pump_out_of_service: "泵况过低停产", no_water_crew: "缺水务工",
  no_repair_crew: "缺机修工", insufficient_materials: "维修缺料", water_power_denied: "净水缺电",
  repair_power_denied: "维修缺电", water_production: "饮水入库", water_quality_changed: "水质变化", algae_bloom: "夏季蓝藻水华", food_production: "农田收成入库", fish_landed: "近岸渔获入库",
  granary_wear: "仓体损耗", granary_no_wood: "粮仓维护缺木材", granary_wood_used: "粮仓维护领用木材",
  granary_repaired: "粮仓维护完成", granary_spoilage: "完全变质报废",
  spoiled_food_to_fuel: "报废粮转入燃料库",
  granary_overflow: "粮食留在仓外", risky_food_rations: "优先配给劣化粮食", foodborne_illness: "食源性不适",
  waterborne_illness: "水源性不适",
  seedbed_irrigation: "晴天灌溉苗床", irrigation_reserved: "余水不足，优先保障饮水",
  no_food_crew: "缺农渔工", salvage_recovery: "回收搬运", wood_regrowth: "林地木料自然补充", pump_wear: "水泵磨损",
  pump_repaired: "水泵修复", repair_materials: "维修耗材", repair_substitute: "改用代用件维修", rations_distributed: "分发配给",
  food_shortfall: "食物短缺", water_shortfall: "饮水短缺", work: "工作疲劳", rest: "休息",
  food_received: "进食", food_missed: "缺餐", water_received: "饮水", water_missed: "缺水",
  work_injury: "工伤", hunger_damage: "饥饿影响健康", thirst_damage: "缺水影响健康",
  kit_preparation: "开始备制医用耗材包", kit_cloth_used: "领用清洁布料", kit_antiseptic_used: "领用消毒用品",
  kit_water_used: "拨出备制用水", kits_produced: "医用耗材包入库",
  clinic_supplies: "领出医用耗材包", medical_treatment: "完成治疗", clinic_waiting: "仍有患者待诊",
  resident_died: "居民死亡", rest_recovery: "休息恢复",
};
const subjectNames = { council: "聚落议事会", cooperative: "生产合作社", exchange: "物资交换组", grid: "微电网", "fuel-yard": "燃料棚", pump: "净水设施", farm: "农田", fishery: "渔点", archive: "掩体档案室", salvage: "残料场", clinic: "卫生站", settlement: "聚落", granary: "粮仓" };
const accountNames = { food: "食物", water: "饮水", fuel: "燃料", wood: "木材", steel: "钢材", field_wood: "现场木料", field_steel: "现场废钢", power: "电力", medicine: "医用耗材包", cloth: "清洁布料", antiseptic: "消毒用品" };

const radius = 39;
const hexWidth = Math.sqrt(3) * radius;
const margin = 14;
const mapWidth = Math.ceil((layout[0].length + 0.5) * hexWidth + margin * 2);
const mapHeight = 2 * radius + (layout.length - 1) * 1.5 * radius + margin * 2;
byId("map").style.setProperty("--map-ratio", mapWidth / mapHeight);
const tiles = new Map();
for (const [row, sequence] of layout.entries()) {
  for (const [col, type] of [...sequence].entries()) {
    const id = `${col},${row}`;
    const q = col - Math.floor(row / 2);
    tiles.set(id, {
      id, col, row, q, type, site: sites[id],
      former: formerPlaces[formerLayout[row][col]],
      district: Object.values(districts).find(({ cells }) => cells.includes(id)),
      x: margin + hexWidth * (0.5 + col + (row % 2) / 2),
      y: margin + radius + row * radius * 1.5,
    });
  }
}

const directions = [
  [1, 0], [1, -1], [0, -1], [-1, 0], [-1, 1], [0, 1],
];
function neighbor(tile, [dq, dr]) {
  const row = tile.row + dr;
  return tiles.get(`${tile.q + dq + Math.floor(row / 2)},${row}`);
}

const canvas = byId("terrain");
const density = Math.min(2, window.devicePixelRatio || 1);
canvas.width = Math.ceil(mapWidth * density);
canvas.height = Math.ceil(mapHeight * density);
const context = canvas.getContext("2d");
context.scale(density, density);

function appendHex(ctx, x, y, size) {
  ctx.moveTo(x, y - size);
  for (let vertex = 1; vertex < 6; vertex++) {
    const angle = (vertex - 1.5) * Math.PI / 3;
    ctx.lineTo(x + Math.cos(angle) * size, y + Math.sin(angle) * size);
  }
  ctx.closePath();
}

function hex(ctx, x, y, size = radius) {
  ctx.beginPath();
  appendHex(ctx, x, y, size);
}

function stroke(ctx, points, color, width = 1) {
  ctx.beginPath();
  for (const [x, y] of points) ctx.lineTo(x, y);
  ctx.lineWidth = width;
  ctx.strokeStyle = color;
  ctx.stroke();
}

function drawOuterGrid(map, viewport) {
  const scale = map.offsetWidth / mapWidth;
  if (scale === 0) return;
  const width = Math.max(map.offsetWidth, viewport.clientWidth) + 4;
  const height = Math.max(map.offsetHeight, viewport.clientHeight) + 4;
  const gutterX = (width - map.offsetWidth) / (2 * scale);
  const gutterY = (height - map.offsetHeight) / (2 * scale);
  const grid = byId("map-grid");
  grid.width = Math.ceil(width * density);
  grid.height = Math.ceil(height * density);
  grid.style.width = `${width}px`;
  grid.style.height = `${height}px`;

  const gridContext = grid.getContext("2d");
  gridContext.scale(scale * density, scale * density);
  gridContext.translate(gutterX, gutterY);
  const rows = Math.ceil(gutterY / (radius * 1.5)) + 1;
  const columns = Math.ceil(gutterX / hexWidth) + 1;
  gridContext.beginPath();
  for (let row = -rows; row < layout.length + rows; row++) {
    for (let col = -columns; col < layout[0].length + columns; col++) {
      if (tiles.has(`${col},${row}`)) continue;
      const x = margin + hexWidth * (0.5 + col + (row & 1) / 2);
      const y = margin + radius + row * radius * 1.5;
      appendHex(gridContext, x, y, radius);
    }
  }
  gridContext.strokeStyle = "#52636565";
  gridContext.lineWidth = 1 / scale;
  gridContext.stroke();
}

function terrainTile(ctx, tile) {
  const { x, y, type, col, row } = tile;
  let seed = (row + 1) * 13649 + (col + 1) * 349;
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
  ctx.save();
  ctx.translate(x, y);
  hex(ctx, 0, 0);
  ctx.clip();
  ctx.fillStyle = terrain[type][1];
  ctx.fillRect(-radius, -radius, radius * 2, radius * 2);
  const light = ctx.createLinearGradient(-35, -39, 30, 38);
  light.addColorStop(0, "#e5d4a933");
  light.addColorStop(.55, "#00000000");
  light.addColorStop(1, "#03182066");
  ctx.fillStyle = light;
  ctx.fillRect(-radius, -radius, radius * 2, radius * 2);

  if ("wm".includes(type)) {
    ctx.lineCap = "round";
    for (let wave = 0; wave < 22; wave++) {
      const dx = random() * 79 - 40;
      const dy = random() * 79 - 40;
      const span = 4 + random() * 19;
      stroke(ctx, [[dx, dy], [dx + span, dy - 2]], type === "w" ? "#abd5cb35" : "#c1d39748");
    }
    if (type === "m") {
      for (let reed = 0; reed < 12; reed++) {
        const dx = random() * 72 - 36;
        const dy = random() * 74 - 37;
        stroke(ctx, [[dx - 3, dy - 6], [dx, dy], [dx + 2, dy - 8]], "#344731a0");
      }
    }
  } else if (type === "x") {
    for (let stripe = -65; stripe < 75; stripe += 13) {
      stroke(ctx, [[stripe - 55, -45], [stripe + 55, 45]], "#eab35b85", 3);
    }
    for (let rock = 0; rock < 18; rock++) {
      const dx = random() * 74 - 37;
      const dy = random() * 80 - 40;
      stroke(ctx, [[dx - 7, dy + 1], [dx, dy - 3], [dx + 3, dy + 2]], "#342e25b0", 2.3);
    }
  } else if ("hf".includes(type)) {
    if (type === "h") {
      for (let band = -30; band <= 22; band += 8) {
        ctx.beginPath();
        ctx.moveTo(-43, band + 18);
        ctx.bezierCurveTo(-10, band - 12, 2, band - 18, 42, band + 8);
        ctx.strokeStyle = "#d6ce9c4d";
        ctx.lineWidth = 1.15;
        ctx.stroke();
      }
    }
    for (let tree = 0; tree < (type === "f" ? 33 : 19); tree++) {
      const dx = random() * 76 - 38;
      const dy = random() * 82 - 41;
      const size = 2 + random() * 4;
      ctx.fillStyle = tree % 3 ? "#203b30d9" : "#9ba777a8";
      ctx.beginPath();
      ctx.moveTo(dx, dy - size);
      ctx.lineTo(dx + size * .62, dy + size);
      ctx.lineTo(dx - size * .62, dy + size);
      ctx.fill();
    }
  } else if (type === "a") {
    for (let field = -38; field <= 46; field += 6) {
      stroke(ctx, [[-46, field + 7], [-10, field - 4], [46, field - 9]], field % 3 ? "#c9bf786f" : "#3d5035a6", 2);
    }
  } else if ("urs".includes(type)) {
    ctx.rotate(type === "r" ? -.21 : .19);
    for (let street = -34; street <= 42; street += 17) {
      stroke(ctx, [[-43, street], [43, street]], type === "r" ? "#406775b0" : "#403d37a0", 2);
      stroke(ctx, [[street, -43], [street, 43]], "#322f2c55", 1.4);
      for (let block = -34; block <= 33; block += 16) {
        const dx = block + 3 + random() * 3;
        const dy = street + 3;
        const w = 4 + random() * 8;
        const h = 4 + random() * 7;
        if (type === "r" && random() < .42) continue;
        ctx.fillStyle = "#24292998";
        ctx.fillRect(dx + 2, dy + 2, w, h);
        ctx.fillStyle = type === "s" ? "#c0a074" : "#a6a09591";
        ctx.fillRect(dx, dy, w, h);
      }
    }
  } else {
    stroke(ctx, [[-42, 4], [-18, 2], [13, -8], [42, -7]], "#b8a68075", 4);
    stroke(ctx, [[-42, 4], [-18, 2], [13, -8], [42, -7]], "#584f43a6", 1);
  }
  for (let grain = 0; grain < 100; grain++) {
    ctx.fillStyle = grain % 2 ? "#f0efc823" : "#0a191b36";
    const size = 0.6 + random() * 1.5;
    ctx.fillRect(random() * 80 - 40, random() * 80 - 40, size, size);
  }
  ctx.restore();
  hex(ctx, x, y, radius - 0.4);
  ctx.strokeStyle = "#17241f8c";
  ctx.lineWidth = 1;
  ctx.stroke();
}

// Cache the atlas artwork. Selection redraws only the highlight layer.
const atlas = document.createElement("canvas");
atlas.width = canvas.width;
atlas.height = canvas.height;
const base = atlas.getContext("2d");
base.scale(density, density);
for (const tile of tiles.values()) terrainTile(base, tile);

function route(ids, color, width) {
  stroke(base, ids.map((id) => {
    const { x, y } = tiles.get(id);
    return [x, y];
  }), color, width);
}

route(["3,4", "4,4", "5,4", "6,4", "7,4", "8,4", "9,4", "10,4", "11,4", "12,3", "13,3"], "#2a322abc", 8);
route(["3,4", "4,4", "5,4", "6,4", "7,4", "8,4", "9,4", "10,4", "11,4", "12,3", "13,3"], "#c3b18c8c", 3);
route(["4,3", "4,4", "5,5", "6,5", "7,6"], "#ceb58d95", 3);
route(["7,6", "8,6"], "#d1c2a97c", 5);
route(["9,6", "10,5", "11,5"], "#c0b4a175", 4);

function outline(tile, sameZone, color, width) {
  // Each axial direction points toward the corresponding hex edge.
  const edges = [1, 0, 5, 4, 3, 2];
  const edgePoint = (index) => {
    const angle = (index - 1.5) * Math.PI / 3;
    return [tile.x + Math.cos(angle) * (radius - 0.8), tile.y + Math.sin(angle) * (radius - 0.8)];
  };
  directions.forEach((direction, index) => {
    if (sameZone(neighbor(tile, direction))) return;
    const edge = edges[index];
    stroke(base, [edgePoint(edge), edgePoint(edge + 1)], color, width);
  });
}

for (const tile of tiles.values()) {
  if (tile.district) {
    hex(base, tile.x, tile.y);
    base.fillStyle = tile.district.color + "32";
    base.fill();
    outline(tile, (other) => other?.district === tile.district, tile.district.color, 2);
  } else if (tile.type === "x") {
    base.setLineDash([5, 3]);
    outline(tile, (other) => other?.type === "x", "#ffc880", 2.8);
    base.setLineDash([]);
  }
}

function mapLabel(text, x, y, color, font) {
  context.font = font;
  context.lineJoin = "round";
  context.lineWidth = 4.5;
  context.strokeStyle = "#101814d8";
  context.strokeText(text, x, y);
  context.fillStyle = color;
  context.fillText(text, x, y);
}

function drawMapLabels() {
  context.textAlign = "center";
  const font = '500 18px "Aftermath Han", sans-serif';
  mapLabel("惠  山", 377, 80, "#e0ece4cc", font);
  mapLabel("太  湖", 139, 514, "#e0ece4cc", font);
  mapLabel("蠡  湖", 490, 566, "#e0ece4cc", font);
  mapLabel("滨 湖 旧 城", 855, 136, "#eee0cfcc", font);
  mapLabel("西线封锁区", 111, 178, "#fff0b9", '900 15px "Aftermath Han", sans-serif');
}

// Markers are drawn with a heavy stroke, so most outlines are straight lines
// only: mitred corners stay crisp where round joins used to blur into blobs.
// The water drop and the fish keep their curved silhouettes and round joins of
// their own. Parallel lines sit at least 4 units apart so the 2.8 unit stroke
// cannot close the gap, and no point reaches past 2.2..21.8 so no stroke spills.
const siteIcons = {
  decision: '<path d="M5 4h10l4 4v12H5z"/><path d="m8 12 2.5 2.5L16 9"/>',
  grid: '<path d="M14.5 2.6 5.4 12.8h5.3L9.4 21.4l9.2-10.2h-5.3z" fill="currentColor" stroke="none"/>',
  fuel: '<path d="M4.5 10h13l3 3v7.5h-16z"/><path d="M8 10V5h5v5"/><path d="M19 10V6h2"/>',
  archive: '<path d="M3.5 6.5h6l2 2.5h9v11h-17z"/><path d="M7.5 13h6.5M7.5 17h4"/>',
  settlement: '<path d="M3.5 20.5v-10L12 3.5l8.5 7v10h-6v-6h-5v6z"/><path d="M14.8 6.9V3.2h3.4v6.2"/>',
  granary: '<path d="M4 20.5V10.5L7 6h10l3 4.5v10z"/><path d="M8.5 20.5V14h7v6.5"/><path d="M4 10.5h16"/>',
  clinic: '<path d="M9.5 3.5h5v6h6v5h-6v6h-5v-6h-6v-5h6z"/>',
  pump: '<path stroke-linecap="round" stroke-linejoin="round" d="M12 3C12 3 6 10 6 14.2a6 6 0 0 0 12 0C18 10 12 3 12 3z"/><path stroke-linecap="round" d="M9.5 14.8a2.6 2.6 0 0 0 2.5 2.4"/>',
  farm: '<path d="M12 21V11"/><path d="M11.5 12.5 4.5 8.5l3-5.5 4.5 4z" fill="currentColor" stroke="none"/><path d="M12.5 12.5 19.5 8.5l-3-5.5-4.5 4z" fill="currentColor" stroke="none"/>',
  fishery: '<path stroke-linecap="round" stroke-linejoin="round" d="M20.5 12c-3.2-4-7.2-5-11-3-1.8 1-3.3 2.3-4.5 3 1.2.7 2.7 2 4.5 3 3.8 2 7.8 1 11-3z"/><path stroke-linecap="round" stroke-linejoin="round" d="M5 12 2.5 9.5v5z"/><circle cx="15.5" cy="11.2" r="1" fill="currentColor" stroke="none"/>',
  pier: '<path d="M3.5 7.5h17"/><path d="M6.5 7.5V15M12 7.5v7.5M17.5 7.5V15"/><path d="M2.5 20.5 6.5 18l4 2.5 4-2.5 4 2.5 3-1.7"/>',
  salvage: '<path d="M4 5h16v15H4z"/><path d="M4 5l16 15M20 5L4 20"/>',
  trail: '<path d="M2.5 19.5 8 8.5l4 6.5 3.5-6 6 10.5z"/>',
  hazard: '<path fill-rule="evenodd" d="M12 2.4 21.8 20.8H2.2z M11.2 8.2h1.6v5.8h-1.6z M11.2 16.2h1.6v1.9h-1.6z" fill="currentColor" stroke="none"/>',
};

function createIcon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  // Butted caps and mitred joins read as a compass-and-ruler drawing; the
  // heavier a stroke gets, the more the round style used to soften the shape.
  svg.setAttribute("stroke-linecap", "butt");
  svg.setAttribute("stroke-linejoin", "miter");
  svg.setAttribute("stroke-miterlimit", "3");
  svg.innerHTML = siteIcons[name] ?? siteIcons.grid;
  return svg;
}

let selected = tiles.get("5,5");
let dayIndex = 0;
let timer = null;
const dayInput = byId("day");
const allEvents = byId("all-events");
const dossierPanel = byId("tile-dossier");
const dossierClose = byId("dossier-close");
dayInput.max = frames.length;
byId("seed-label").textContent = `种子 ${byId("seed").textContent.trim()}`;

for (const tile of tiles.values()) {
  const button = element("button");
  button.type = "button";
  button.className = "hex";
  button.style.cssText = `left:${tile.x / mapWidth * 100}%;top:${tile.y / mapHeight * 100}%;width:${hexWidth / mapWidth * 100}%;height:${2 * radius / mapHeight * 100}%`;
  tile.label = `${tile.site?.name ?? terrain[tile.type][0]} · 战前：${tile.former} · Q${tile.q} R${tile.row}`;
  button.title = tile.label;
  button.setAttribute("aria-label", tile.label);
  if (tile.site) {
    const icon = element("span");
    icon.className = "marker";
    icon.append(createIcon(tile.site.icon));
    icon.setAttribute("aria-hidden", "true");
    button.append(icon);
  }
  button.addEventListener("click", () => select(tile));
  button.addEventListener("keydown", (event) => {
    const index = { ArrowRight: 0, ArrowUp: event.shiftKey ? 2 : 1,
      ArrowLeft: 3, ArrowDown: event.shiftKey ? 4 : 5 }[event.key];
    if (index === undefined) return;
    event.preventDefault();
    const next = neighbor(tile, directions[index]);
    if (next) {
      select(next);
      next.button.focus({ preventScroll: true });
    }
  });
  tile.button = button;
  byId("tiles").append(button);
}

for (const [label, color] of Object.values(terrain)) {
  const item = element("li");
  const swatch = element("i");
  swatch.style.setProperty("--terrain", color);
  item.append(swatch, label);
  byId("legend").append(item);
}

for (const { label, icon, color } of Object.values(districts)) {
  const item = element("li");
  const badge = element("span");
  badge.className = "legend-badge";
  badge.style.setProperty("--terrain", color);
  badge.append(createIcon(icon));
  item.append(badge, label);
  byId("district-legend").append(item);
}
const alertItem = element("li");
const alertSwatch = element("i");
alertSwatch.className = "alert-swatch";
alertItem.append(alertSwatch, "异常 / 待诊");
byId("district-legend").append(alertItem);

function renderSelection() {
  context.clearRect(0, 0, mapWidth, mapHeight);
  context.drawImage(atlas, 0, 0, mapWidth, mapHeight);
  drawMapLabels();
  hex(context, selected.x, selected.y, radius - 1.4);
  context.fillStyle = "#9fe6ed42";
  context.fill();
  context.strokeStyle = "#b9f3f7";
  context.lineWidth = 2.8;
  context.stroke();
  for (const tile of tiles.values()) {
    tile.button.setAttribute("aria-pressed", String(tile === selected));
    tile.button.tabIndex = tile === selected ? 0 : -1;
  }
}

function select(tile) {
  selected = tile;
  // Closing the record is temporary: choosing a tile brings it back.
  dossierPanel.hidden = false;
  renderSelection();
  renderInspector();
}

function stat(label, value) {
  const wrapper = element("div");
  wrapper.append(element("dt", label), element("dd", value));
  return wrapper;
}

function renderInspector(targetId) {
  const { site, type, q, row } = selected;
  const [name, , description] = terrain[type];
  byId("coords").textContent = `Q${q} / R${row}`;
  byId("region").textContent = [site?.region ?? name, selected.district?.label].filter(Boolean).join(" · ");
  byId("former-name").textContent = selected.former;
  byId("tile-name").textContent = site?.name ?? name;
  byId("description").textContent = site?.text ?? description;
  const photoPlace = site && photos.has(site.name) ? site.name
    : photos.has(selected.former) ? selected.former : undefined;
  const photo = photoPlace ? photos.get(photoPlace) : undefined;
  const figure = byId("tile-photo");
  figure.hidden = !photo;
  if (photo) {
    const image = byId("tile-photo-image");
    if (image.dataset.place !== photoPlace) {
      image.src = photo;
      image.alt = `${photoPlace} 近期影像`;
      image.dataset.place = photoPlace;
      figure.style.setProperty("--photo", `url("${photo}")`);
    }
    if (image.complete) applyPhotoOrientation();
    byId("tile-photo-caption").textContent = `${photoPlace} · 近期影像 · 点击放大`;
  }

  const frame = frames[dayIndex];
  const dailyEvents = eventsByDay.get(frame.day) ?? [];
  const local = dailyEvents.filter((event) => (site?.subject === "granary" && event.account === "food")
    || plotLocations.get(event.subject) === selected.id
    || event.subject === site?.subject
    || (site?.subject === "granary" && event.reason === "spoiled_food_to_fuel")
    || (site?.subject === "clinic" && [
      "medical_treatment", "foodborne_illness", "waterborne_illness", "work_injury", "hunger_damage", "thirst_damage",
    ].includes(event.reason)));
  const plot = plotsByDay[dayIndex].get(selected.id);
  const plotStats = plot ? [
    ["现场木料", measure("wood", plot.wood)],
    ["现场废钢", measure("steel", plot.steel)],
    ["可回收", plot.accessible ? "可进入" : "封锁"],
    ...(plot.capacity ? [["木料再生", "林地可自然补充"]] : []),
  ] : [];
  const flowAmount = (account, change, reason) => local
    .filter((event) => event.account === account && event.change === change
      && (reason === undefined || event.reason === reason))
    .reduce((total, event) => total + Math.abs(+event.after - +event.before), 0);
  const flow = (account, change, reason) => measure(account, flowAmount(account, change, reason));
  const statistics = {
    pump: [["泵况", `${frame.pump}%`], ["净水入库", flow("water", "produced")],
      ["水质", frame.water > 0 ? `${frame.waterQuality}/100` : "—"],
      ["净水用电", flow("power", "consumed", "water_load")], ["维修用电", flow("power", "consumed", "repair_load")],
      ["维修木材", flow("wood", "consumed")], ["维修钢材", flow("steel", "consumed")]],
    grid: [["额定容量", powerRate(frame.gridRated)], ["当日上线容量", powerRate(gridOnline(frame))],
      ["请求负荷", powerRate(frame.gridRequested)], ["已供负荷", powerRate(frame.gridDelivered)],
      ["未供负荷", powerRate(frame.gridRequested - frame.gridDelivered)],
      ["电网压力", gridPressure(frame)], ["电压", `${frame.gridVoltage} V`],
      ["频率", `${(frame.gridFrequency / 100).toFixed(2)} Hz`],
      ["在线电源", frame.gridBackup > 0 ? "逆变器 + 备用机组" : "逆变器"], ["可再生发电", measure("power", frame.gridRenewable)],
      ["备用发电", measure("power", frame.gridBackup)], ["实际用电", measure("power", frame.gridDelivered)],
      ["日终余电清零", flow("power", "consumed", "power_expired")], ["耗燃料", flow("fuel", "consumed")]],
    "fuel-yard": [["今日燃料入库", flow("fuel", "produced")], ["报废粮转燃料", flow("fuel", "produced", "spoiled_food_to_fuel")],
      ["燃料库存", measure("fuel", frame.fuel)]],
    archive: [["档案终端用电", flow("power", "consumed")]],
    clinic: [["当日食源性不适", `${local.filter((event) => event.reason === "foodborne_illness").length} 人`],
      ["当日水源性不适", `${local.filter((event) => event.reason === "waterborne_illness").length} 人`],
      ["当日工伤", `${local.filter((event) => event.reason === "work_injury").length} 人`],
      ["每日接诊上限", `${frame.medicalCapacity} 人`], ["当日已治疗", `${frame.treated} 人`],
      ["当日晚间待诊", `${frame.waitingForCare} 人`], ["当日仍可接诊", `${spareCare(frame)} 人`],
      ["消耗医用耗材包", flow("medicine", "consumed")], ["今日备制", flow("medicine", "produced")],
      ["剩余医用耗材包", measure("medicine", frame.medicine)], ["清洁布料库存", measure("cloth", frame.cloth)],
      ["消毒用品库存", measure("antiseptic", frame.antiseptic)],
      ["备制用水", flow("water", "consumed", "kit_water_used")]],
    granary: [["食物库存", measure("food", frame.food)], ["今日入库", flow("food", "produced")],
      ["配给出库", flow("food", "consumed", "rations_distributed")],
      ["有效仓容", measure("food", frame.granaryCapacity)], ["仓况", `${frame.granaryCondition}%`],
      ["仓外粮食", measure("food", Math.max(0, frame.food - frame.granaryCapacity))],
      ["劣化配给", measure("food", dailyEvents.find((event) => event.reason === "risky_food_rations")?.after ?? 0)],
      ["今日转燃料", flow("fuel", "produced", "spoiled_food_to_fuel")], ["维护木材", flow("wood", "consumed", "granary_wood_used")],
      ["可供配给", frame.population > 0 ? `${Math.floor(frame.food / frame.population)} 日` : "—"]],
    farm: [["实际到岗", `${dailyEvents.find((event) => event.change === "food_crew")?.before ?? 0} / 13 人`], ["农田收成", flow("food", "produced")], ["苗床用水", flow("water", "consumed")]],
    fishery: [["实际到岗", `${dailyEvents.find((event) => event.change === "food_crew")?.after ?? 0} / 7 人`], ["近岸渔获", flow("food", "produced")]],
    salvage: [["回收木材", flow("wood", "transfer_in")], ["回收钢材", flow("steel", "transfer_in")]],
    settlement: [["食物配给", flow("food", "consumed")], ["饮水配给", flow("water", "consumed")],
      ["食物缺给", measure("food", frame.foodShortfall)], ["饮水缺给", measure("water", frame.waterShortfall)],
      ["公共灶用电", flow("power", "consumed")], ["接班 / 完工", `${frame.accepted} / ${frame.completed}`],
      ["无人接班", frame.unfilled]],
  };
  byId("facility-stats").replaceChildren(...[...(statistics[site?.subject] ?? []), ...plotStats].map(([key, value]) => stat(key, value)));
  renderEvents(allEvents.checked ? dailyEvents : local, targetId);
}

const warningEvents = new Set([
  "pump_out_of_service", "no_water_crew", "no_repair_crew", "insufficient_materials",
  "water_power_denied", "repair_power_denied", "food_shortfall", "water_shortfall",
  "clinic_waiting", "work_injury", "resident_died",
  "granary_no_wood", "granary_spoilage", "foodborne_illness", "waterborne_illness",
]);

const eventCategories = {
  grid: "电网", pump: "水务", farm: "农田", fishery: "渔业", "fuel-yard": "燃料",
  archive: "档案", salvage: "回收", clinic: "医疗", settlement: "聚落", granary: "粮仓",
};

function eventCategory(event) {
  if (warningEvents.has(event.reason)) return "警报";
  if (organizations.has(event.subject) || event.reason.startsWith("backup_vote_") || event.reason.startsWith("food_vote_") || event.reason === "fishing_priority_proposed" || event.reason === "backup_limit_proposed" || event.reason === "exchange_signed_stock") return "组织";
  if (event.reason === "clear" || event.reason === "rain") return "气象";
  if (event.reason === "medical_treatment" || event.reason === "foodborne_illness") return "医疗";
  if (plotLocations.has(event.subject)) return event.reason === "wood_regrowth" ? "林地" : "回收";
  return eventCategories[event.subject] ?? "居民";
}

function renderEvents(entries, targetId) {
  const list = byId("event-list");
  list.replaceChildren();
  if (!entries.length) {
    const empty = element("li", selected.site?.subject ? "当天无地块事件。" : "此地块没有模拟设施。");
    empty.className = "empty";
    list.append(empty);
    return;
  }
  let target;
  for (const event of entries) {
    const item = element("li");
    const category = element("span", eventCategory(event));
    category.className = "event-category";
    item.classList.toggle("warning", warningEvents.has(event.reason));
    const summary = element("div");
    summary.className = "event-summary";
    const time = element("time", event.time);
    time.dateTime = event.time;
    summary.append(element("span", reasonNames[event.reason] ?? event.reason));
    const plotTile = tiles.get(plotLocations.get(event.subject));
    const subject = plotTile ? (plotTile.site?.name ?? plotTile.former)
      : subjectNames[event.subject] ?? residentNames.get(event.residentId) ?? event.subject;
    let detail = `#${event.id} · ${subject}`;
    if (event.account) {
      const delta = +event.after - +event.before;
      const amount = measure(event.account, Math.abs(delta));
      detail += ` · ${accountNames[event.account]} ${delta < 0 ? "−" : "+"}${amount}`;
      detail += ` · 余 ${measure(event.account, event.plotBalance ?? +event.after)}`;
    } else if (event.change === "food_quantity") {
      detail += ` · ${measure("food", Number(event.after))}`;
    } else if (event.change === "food_crew") {
      detail += ` · 农田 ${event.before} 人 / 渔点 ${event.after} 人`;
    } else if (event.change === "harvest_rates") {
      detail += ` · 农田 ${measure("food", +event.before)}/班 · 渔点 ${measure("food", +event.after)}/班`;
    } else if (event.change === "vote_tally") {
      detail += ` · 赞成 ${event.before} / 反对 ${event.after}`;
    } else if (event.change === "proposal_stock") {
      detail += ` · 燃料 ${measure("fuel", +event.before)} · 储水 ${measure("water", +event.after)}`;
    } else if (event.change === "proposal_condition") {
      detail += ` · 泵况 ${event.before}% · 水质 ${event.after}/100`;
    } else if (event.change === "backup_quota") {
      detail += ` · 备用发电 ${event.after} 档`;
    } else if (event.change === "familiarity") {
      detail += ` · 搭班 ${residentNames.get(event.relatedResidentId)}`;
    } else if (event.change === "crew_priority") {
      detail += ` · 对照排序人选 ${residentNames.get(event.relatedResidentId)}`;
    } else if (event.change === "backup_fuel") {
      detail += ` · 可用 ${measure("fuel", +event.before)} / 需 ${measure("fuel", +event.after)}`;
    } else if (event.change) {
      detail += ` · ${event.before} → ${event.after}`;
    }
    summary.append(element("small", detail));
    item.append(time, category, summary);
    if (event.cause) {
      const cause = element("button", `↖ #${event.cause}`);
      cause.className = "cause";
      cause.type = "button";
      cause.setAttribute("aria-label", `查看原因事件 ${event.cause}`);
      cause.addEventListener("click", () => {
        const parent = events.get(event.cause);
        if (!parent) return;
        stop();
        allEvents.checked = true;
        showDay(parent.day - 1, parent.id);
      });
      summary.append(cause);
    }
    if (event.id === targetId) {
      item.classList.add("target");
      item.tabIndex = -1;
      target = item;
    }
    list.append(item);
  }
  list.scrollTop = 0;
  if (target) {
    list.scrollTop = target.offsetTop - list.firstElementChild.offsetTop;
    target.focus({ preventScroll: true });
  }
}

const resourceColors = {
  population: "#b5cfcb", food: "#a9b69a", fuel: "#ac9b88", wood: "#a99c87",
  steel: "#94a4af", medicine: "#b4a1af",
};
const stockMax = Object.fromEntries(Object.keys(resourceColors).map((account) =>
  [account, Math.max(1, ...frames.map((frame) => frame[account]))],
));

const resourceRows = Object.entries(resourceColors).map(([account, color]) => {
  const row = element("div");
  row.className = "metric-row";
  row.style.setProperty("--bar", color);
  const label = account === "population" ? "人口" : accountNames[account];
  const bar = element("span");
  bar.className = "stock-bar";
  bar.setAttribute("role", "meter");
  bar.setAttribute("aria-label", `${label}占本次峰值`);
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", stockMax[account]);
  const content = element("dl");
  content.className = "metric-content";
  content.setAttribute("aria-hidden", "true");
  const amount = element("dd");
  content.append(element("dt", label), amount);
  row.append(bar, content);
  byId("metrics").append(row);
  return { account, row, bar, amount };
});

function createUsageRows(name, entries) {
  return entries.map(([reason, label, color]) => {
    const segment = element("span");
    segment.style.setProperty("--segment", color);
    byId(`${name}-stack`).append(segment);
    const entry = element("div");
    entry.style.setProperty("--segment", color);
    const value = element("dd");
    entry.append(element("dt", label), value);
    byId(`${name}-breakdown`).append(entry);
    return { reason, segment, value };
  });
}

const powerRows = createUsageRows("power", [
  ["water_load", "净水", "#e5d9a5"], ["repair_load", "维修", "#a99a73"],
  ["kitchen_load", "公灶", "#c9bea0"], ["archive_load", "档案", "#81785e"],
  ["power_expired", "未用", "#4c4940"],
]);

const waterRows = createUsageRows("water", [
  ["rations_distributed", "居民饮水", "#a6dbe0"], ["seedbed_irrigation", "灌溉", "#9ab19c"],
  ["kit_water_used", "医疗", "#8b9ab9"],
]);

function usageTotals(daily, account) {
  const totals = new Map();
  for (const event of daily) {
    if (event.account !== account || event.change !== "consumed") continue;
    const consumed = Number(event.before) - Number(event.after);
    totals.set(event.reason, (totals.get(event.reason) ?? 0) + consumed);
  }
  return totals;
}

function renderUsage(rows, totals, account, capacity) {
  for (const { reason, segment, value } of rows) {
    const amount = totals.get(reason) ?? 0;
    segment.style.setProperty("--share", amount / Math.max(1, capacity));
    value.textContent = measure(account, amount);
  }
}

function spareCare(frame) {
  return Math.min(frame.medicalCapacity - frame.treated - frame.kitsPrepared, frame.medicine);
}

function gridOnline(frame) {
  return frame.gridRenewable + frame.gridBackup;
}

function powerRate(amount) {
  // All capacity and load estimates refer to one modeled dispatch hour.
  return `${ordinaryNumber.format(amount * units.power.scale / 10)} kW`;
}

function gridPressure(frame) {
  const online = gridOnline(frame);
  return online > 0 ? `${Math.round(frame.gridRequested / online * 100)}%` : "停供";
}

function telemetryTone(color, brightness) {
  const level = Math.max(0, Math.min(1, brightness));
  return `color-mix(in srgb, ${color} ${(level * 100).toFixed(2)}%, #121516)`;
}

function waterTone(quality) {
  return telemetryTone("#99cbdc", Math.pow(quality / 100, 1.45));
}

function voltageTone(voltage) {
  return telemetryTone("#f3e9c2", (voltage - 195) / 59);
}

byId("water-reference").style.setProperty("--reference-color", waterTone(80));
byId("grid-reference").style.setProperty("--reference-color", voltageTone(220));

function renderGridSignal(frame) {
  const frequency = frame.gridFrequency / 100;
  byId("grid-fill").style.setProperty("--reading-color", voltageTone(frame.gridVoltage));
  const wave = byId("grid-wave");
  if (frequency === 0) {
    wave.setAttribute("d", "");
    return;
  }
  const tint = frequency < 50 ? "#ae8cf2" : "#efcd71";
  const strength = Math.min(1, Math.abs(frequency - 50) / .55);
  wave.style.setProperty("--wave-color",
    `color-mix(in srgb, ${tint} ${(strength * 100).toFixed(2)}%, #dad9d5)`);
  // A 100 ms view shows five periods at the nominal 50 Hz.
  const points = Array.from({ length: 129 }, (_, index) => {
    const part = index / 128;
    const y = 14 - 8 * Math.sin(2 * Math.PI * frequency * .1 * part);
    return `${index === 0 ? "M" : "L"}${(320 * part).toFixed(1)} ${y.toFixed(2)}`;
  });
  wave.setAttribute("d", points.join(" "));
}

function renderGrid(frame, daily) {
  const summary = byId("grid-summary");
  const online = gridOnline(frame);
  const overload = frame.gridRequested > online;
  const pressure = gridPressure(frame);
  summary.classList.toggle("overload", overload);
  byId("grid-pressure").textContent = pressure;
  byId("grid-load").textContent = `${ordinaryNumber.format(frame.gridRequested * units.power.scale / 10)} / ${powerRate(online)}`;
  const gauge = byId("grid-gauge");
  gauge.setAttribute("aria-valuemax", Math.max(1, online));
  gauge.setAttribute("aria-valuenow", Math.min(online, frame.gridRequested));
  gauge.setAttribute("aria-valuetext", `电网压力 ${pressure}，请求 ${powerRate(frame.gridRequested)}，上线 ${powerRate(online)}`);
  const fill = online > 0 ? Math.min(1, frame.gridRequested / online) : Number(frame.gridRequested > 0);
  byId("grid-fill").style.setProperty("--level", `${fill * 100}%`);
  renderGridSignal(frame);
  renderUsage(powerRows, usageTotals(daily, "power"), "power", online);
}

function renderWater(frame, daily) {
  const produced = daily.filter((event) => event.account === "water" && event.change === "produced")
    .reduce((total, event) => total + Number(event.after) - Number(event.before), 0);
  const totals = usageTotals(daily, "water");
  const delivered = [...totals.values()].reduce((sum, amount) => sum + amount, 0);
  const requested = frame.waterRequested;
  const pressure = produced > 0 ? Math.round(requested / produced * 100) + "%"
    : requested > 0 ? "停泵" : "—";
  const quality = frame.water > 0 || delivered > 0 ? frame.waterQuality : null;
  byId("water-fill").style.setProperty("--reading-color", waterTone(quality ?? 0));
  const reserveDraw = Math.max(0, delivered - produced);
  const unserved = Math.max(0, requested - delivered);
  byId("water-summary").classList.toggle("overload", requested > produced);
  byId("water-pressure").textContent = pressure;
  byId("water-load").textContent = `${ordinaryNumber.format(requested * units.water.scale / 10)} / ${measure("water", produced)}/日`;
  byId("water-reserve").textContent = `储水 ${measure("water", frame.water)}`;
  byId("water-gap").textContent = `储备补给 ${measure("water", reserveDraw)}`
    + (unserved > 0 ? ` · 缺供 ${measure("water", unserved)}` : "");
  const gauge = byId("water-gauge");
  gauge.setAttribute("aria-valuemax", Math.max(1, produced));
  gauge.setAttribute("aria-valuenow", Math.min(requested, Math.max(1, produced)));
  gauge.setAttribute("aria-valuetext", `供水压力 ${pressure}，需求 ${measure("water", requested)}，净水 ${measure("water", produced)}，水质 ${quality === null ? "—" : `${quality}/100`}`);
  const fill = produced > 0 ? Math.min(1, requested / produced) : Number(requested > 0);
  byId("water-fill").style.setProperty("--level", `${fill * 100}%`);
  renderUsage(waterRows, totals, "water", delivered);
}

const granarySegments = ["granary-storage", "granary-exposed"].map((id) =>
  ["#c0d09a", "#8f9a72", "#5d664c", "#393e32", "#171a17"].map((color) => {
    const segment = element("i");
    segment.style.setProperty("--quality", color);
    byId(id).append(segment);
    return segment;
  }),
);
const exposedScale = Math.max(1, ...foodQuality.map(([, bins]) => bins.reduce((sum, amount) => sum + amount, 0)));

function renderGranary(frame, daily) {
  const loss = daily.find((event) => event.reason === "granary_spoilage");
  const exposed = Math.max(0, frame.food - frame.granaryCapacity);
  const dailyOverflow = Number(daily.find((event) => event.reason === "granary_overflow")?.after ?? 0);
  const spoiled = loss ? Number(loss.before) - Number(loss.after) : 0;
  const recovered = daily.find((event) => event.reason === "spoiled_food_to_fuel");
  const fuel = recovered ? Number(recovered.after) - Number(recovered.before) : 0;
  const profile = foodQuality[dayIndex];
  const risky = profile.reduce((total, bins) => total + bins.slice(1).reduce((sum, amount) => sum + amount, 0), 0);
  byId("granary-summary").classList.toggle("overload", dailyOverflow > 0 || spoiled > 0);
  byId("granary-load").textContent = `仓内 ${measure("food", frame.food - exposed)} / ${measure("food", frame.granaryCapacity)}`;
  byId("granary-condition").textContent = `仓况 ${frame.granaryCondition}%`;
  byId("granary-overflow").textContent = exposed > 0 || dailyOverflow === 0
    ? `仓外 ${measure("food", exposed)}`
    : `当日溢出 ${measure("food", dailyOverflow)}`;
  byId("granary-risk").textContent = `劣化 ${measure("food", risky)}`;
  byId("granary-loss").textContent = `转燃料 ${measure("fuel", fuel)}`;
  byId("granary-gauge").setAttribute("aria-label",
    `仓内 ${measure("food", frame.food - exposed)}，仓外 ${measure("food", exposed)}，劣化 ${measure("food", risky)}`);
  const scales = [frame.granaryCapacity, exposedScale];
  profile.forEach((bins, lane) => bins.forEach((amount, band) => {
    granarySegments[lane][band].style.setProperty("--share", `${amount / scales[lane] * 100}%`);
  }));
  tiles.get("6,4").button.classList.toggle("interrupted", dailyOverflow > 0 || spoiled > 0);
}

function renderOverview(frame) {
  for (const { account, row, bar, amount } of resourceRows) {
    const count = frame[account];
    const quantity = account === "population" ? `${count} 人` : measure(account, count);
    bar.setAttribute("aria-valuenow", count);
    bar.setAttribute("aria-valuetext", quantity);
    row.style.setProperty("--level", `${count / stockMax[account] * 100}%`);
    amount.textContent = quantity;
  }
}

function renderCalendar(frame, targetId) {
  const date = new Date(Date.UTC(2051, 3, frame.day));
  const dateLabel = byId("calendar-date");
  dateLabel.dateTime = date.toISOString().slice(0, 10);
  dateLabel.textContent = dateLabel.dateTime;
  const event = events.get(targetId);
  const time = byId("clock-value");
  time.dateTime = event?.time ?? "23:59";
  time.textContent = `${event ? "事件" : "日终"} ${time.dateTime}`;
  byId("weather").textContent = frame.weather === "rain" ? "阴雨" : "晴";
}

function setHealthState(plant, state, label) {
  const row = byId(`health-${plant}`);
  row.classList.remove("ok", "warn", "fault");
  row.classList.add(state);
  byId(`health-${plant}-state`).textContent = label;
}

function renderHealth(frame, daily) {
  const produced = daily
    .filter((event) => event.account === "water" && event.change === "produced")
    .reduce((total, event) => total + Number(event.after) - Number(event.before), 0);
  const waterFault = frame.pump <= 20;
  const waterWarn = !waterFault && (frame.pump < 60 || frame.waterQuality < 65);
  setHealthState("water", waterFault ? "fault" : waterWarn ? "warn" : "ok",
    waterFault ? "停泵" : waterWarn ? "注意" : "正常");
  const quality = frame.water > 0 || produced > 0 ? `${frame.waterQuality}/100` : "—";
  byId("health-water-detail").textContent = `泵况 ${frame.pump}% · 水质 ${quality}`;
  const online = gridOnline(frame);
  const frequency = frame.gridFrequency / 100;
  const gridFault = frame.gridRequested > online;
  const gridWarn = !gridFault && (frame.gridVoltage < 195 || frame.gridVoltage > 240
    || (frequency > 0 && (frequency < 49.5 || frequency > 50.5)));
  setHealthState("grid", gridFault ? "fault" : gridWarn ? "warn" : "ok",
    gridFault ? "超载" : gridWarn ? "注意" : "正常");
  byId("health-grid-detail").textContent =
    `${frame.gridVoltage} V · ${frequency.toFixed(2)} Hz`;
}

const voteButtons = [
  { prefix: "backup_vote_", button: byId("last-vote") },
  { prefix: "food_vote_", button: byId("last-food-vote") },
];
for (const entry of voteButtons) {
  entry.button.addEventListener("click", () => {
    if (!entry.vote) return;
    stop();
    allEvents.checked = true;
    showDay(entry.vote.day - 1, entry.vote.id);
  });
}

function harvestWeight(daily, reason) {
  const harvest = daily.find((event) => event.reason === reason);
  return measure("food", harvest ? Number(harvest.after) - Number(harvest.before) : 0);
}

if (byId("use-familiarity").textContent === "false") {
  byId("member-rule").textContent = "对照运行采用相同的轮班规则，已关闭近期合作排序。";
}

let selectedMember;
let rosterSignature;
let memberButtons = [];

function renderMemberDetail(day) {
  const panel = byId("member-detail");
  panel.hidden = !selectedMember;
  for (const { id, button } of memberButtons) {
    button.setAttribute("aria-pressed", String(id === selectedMember));
  }
  if (!selectedMember) return;
  byId("member-name").textContent = residentNames.get(selectedMember);
  const partners = [];
  const pairs = relationshipsAt(day);
  let unknown = 0;
  let today = 0;
  for (const [id, name] of residentNames) {
    if (id === selectedMember) continue;
    const pair = pairs.get(pairKey(id, selectedMember));
    const deceased = (deaths.get(id) ?? Infinity) <= day;
    if (!pair) {
      if (!deceased) unknown++;
      continue;
    }
    if (pair.lastDay === day) today++;
    partners.push({ name, deceased, ...pair });
  }
  byId("member-summary").textContent = `今日同班 +${today} 人 · 从未同班 ${unknown} 位在世居民`;
  const list = byId("member-partners");
  list.replaceChildren();
  for (const partner of partners.sort((a, b) => b.lastDay - a.lastDay || b.count - a.count)) {
    const item = element("li");
    item.append(
      element("span", partner.name + (partner.deceased ? "（已故）" : "")),
      element("span", `${partner.count} 次 · 最近第 ${partner.lastDay} 日`),
    );
    list.append(item);
  }
  if (!partners.length) list.append(element("li", "截至当日尚无同班完工记录。"));
  const notes = (eventsByDay.get(day) ?? []).filter((event) =>
    event.change === "crew_priority" && (event.residentId === selectedMember || event.relatedResidentId === selectedMember));
  byId("member-priority").textContent = notes.map((event) =>
    `近期合作影响入选：${residentNames.get(event.residentId)} 入选，对照人选为 ${residentNames.get(event.relatedResidentId)}。`).join(" ");

}

function renderOrganizationMembers(day) {
  const groups = [...organizations].map(([group, members]) =>
    [group, members.filter((id) => (deaths.get(id) ?? Infinity) > day)]);
  for (const [group, members] of groups) {
    let recent = 0;
    for (let i = 0; i < members.length; i++) {
      for (let j = i + 1; j < members.length; j++) {
        const pair = relationshipsAt(day).get(pairKey(members[i], members[j]));
        if (pair && day - pair.lastDay < 14) recent++;
      }
    }
    const possible = members.length * (members.length - 1) / 2;
    byId(`${group}-cooperation`).textContent = `近 14 天同班配对 ${recent}/${possible}`;
  }
  const signature = groups.map(([, members]) => members.join(",")).join("|");
  if (signature !== rosterSignature) {
    rosterSignature = signature;
    memberButtons = [];
    for (const [group, members] of groups) {
      const links = byId(`${group}-members`);
      links.replaceChildren();
      byId(`${group}-members-count`).textContent = `${members.length} 人`;
      for (const id of members) {
        const button = element("button", residentNames.get(id));
        button.type = "button";
        button.className = "member-link";
        button.setAttribute("aria-controls", "member-detail");
        button.addEventListener("click", () => {
          stop();
          selectedMember = id === selectedMember ? undefined : id;
          renderMemberDetail(frames[dayIndex].day);
          if (selectedMember) byId("member-detail").scrollIntoView({ block: "nearest" });
        });
        memberButtons.push({ id, button });
        links.append(button);
      }
      if (!members.length) links.textContent = "—";
    }
    if (!memberButtons.some(({ id }) => id === selectedMember)) selectedMember = undefined;
  }
  renderMemberDetail(day);
}

function renderOrganizations(frame, daily) {
  const policy = daily.find((event) => event.change === "backup_quota");
  byId("backup-policy").textContent = policy ? reasonNames[policy.reason] : "备用发电按常规调度";
  const fishingFirst = daily.some((event) => event.reason === "fishing_priority_effective");
  byId("food-policy").textContent = fishingFirst ? "缺工时渔点优先" : "缺工时农田优先";
  const crew = daily.find((event) => event.change === "food_crew");
  byId("food-output").textContent =
    `农田 ${crew?.before ?? 0}/13 人 · ${harvestWeight(daily, "food_production")} / 渔点 ${crew?.after ?? 0}/7 人 · ${harvestWeight(daily, "fish_landed")}`;
  for (const entry of voteButtons) {
    const vote = voteResults.findLast((event) => event.day <= frame.day && event.reason.startsWith(entry.prefix));
    entry.vote = vote;
    entry.button.hidden = !vote;
    if (vote) {
      entry.button.textContent = `↖ 第 ${vote.day} 日 · ${reasonNames[vote.reason]} · 赞成 ${vote.before} / 反对 ${vote.after}`;
    }
  }
  renderOrganizationMembers(frame.day);
}

const decisionVenue = [...tiles.values()].find((tile) => tile.site?.decisionVenue);
const decisionMarker = byId("decision-marker");
const decisionPanel = byId("decision-panel");
const decisionClose = byId("decision-close");
const decisionCount = element("span");
const decisionIcon = createIcon("decision");
decisionIcon.setAttribute("aria-hidden", "true");
decisionMarker.append(decisionIcon, decisionCount);
decisionMarker.style.left = `${(decisionVenue.x + hexWidth * .22) / mapWidth * 100}%`;
decisionMarker.style.top = `${(decisionVenue.y - radius * .45) / mapHeight * 100}%`;
let decisionDay;

function setDecisionOpen(open, restoreFocus = false) {
  decisionPanel.hidden = !open;
  decisionMarker.setAttribute("aria-expanded", String(open));
  if (open) decisionClose.focus({ preventScroll: true });
  else if (restoreFocus && !decisionMarker.hidden) decisionMarker.focus({ preventScroll: true });
}

decisionMarker.addEventListener("click", () => {
  if (decisionPanel.hidden) stop();
  setDecisionOpen(decisionPanel.hidden, true);
});
decisionClose.addEventListener("click", () => setDecisionOpen(false, true));
decisionPanel.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  event.preventDefault();
  setDecisionOpen(false, true);
});

function resolutionEvidence(vote) {
  const evidence = new Map();
  for (let id = vote.cause; id;) {
    const event = events.get(id);
    if (!event || event.day !== vote.day) break;
    evidence.set(event.reason, event);
    id = event.cause;
  }
  return evidence;
}

function resolutionEffect(vote) {
  const farming = vote.reason.startsWith("food_vote_");
  const passed = vote.reason.endsWith("_passed");
  if (!passed) return farming ? "农渔排班继续按农田优先执行。" : "备用发电继续按常规调度。";
  const nextDay = vote.day + 1;
  const daily = eventsByDay.get(nextDay) ?? [];
  const effective = daily.find((event) => event.cause === vote.id);
  if (farming) {
    const crew = daily.find((event) => event.change === "food_crew");
    return `第 ${nextDay} 日缺工时先排渔点。`
      + (crew ? `实际到岗与产量 · 农田 ${crew.before}/13 人、${harvestWeight(daily, "food_production")}，渔点 ${crew.after}/7 人、${harvestWeight(daily, "fish_landed")}。` : "");
  }
  if (effective?.reason === "backup_reserve_override") return `第 ${nextDay} 日储水触及应急线，恢复备用发电。`;
  return `第 ${nextDay} 日暂停备用发电。`
    + (daily.some((event) => event.reason === "backup_blocked_by_vote") ? "该日有负载因此未获备用电。" : "");
}

function renderMapDecisions(frame, daily) {
  const votes = daily.filter((event) => event.change === "vote_tally");
  if (decisionDay !== frame.day) {
    const restoreFocus = decisionPanel.contains(document.activeElement);
    setDecisionOpen(false, restoreFocus && votes.length > 0);
  }
  decisionDay = frame.day;
  decisionMarker.hidden = !votes.length;
  if (!votes.length) return;
  const label = `${decisionVenue.site.name} · ${votes.length} 项表决`;
  decisionMarker.title = label;
  decisionMarker.setAttribute("aria-label", label);
  decisionCount.textContent = votes.length;
  const content = byId("decision-content");
  content.replaceChildren(element("p", `${decisionVenue.site.name} · 第 ${frame.day} 日`));
  for (const vote of votes) {
    const article = element("article");
    const tally = element("p", `${vote.time} · 赞成 ${vote.before} 票 · 反对 ${vote.after} 票`);
    tally.className = "decision-tally";
    article.append(element("h3", reasonNames[vote.reason]), tally);
    const evidence = resolutionEvidence(vote);
    const fuel = evidence.get("backup_stock_review");
    const pump = evidence.get("backup_service_review");
    const grain = evidence.get("food_vote_stock_review");
    const harvest = evidence.get("food_vote_harvest_review");
    if (fuel) article.append(element("p", `备用燃料 ${measure("fuel", +fuel.before)} · 储水 ${measure("water", +fuel.after)}`));
    if (pump) article.append(element("p", `泵况 ${pump.before}% · 水质 ${pump.after}/100`));
    if (grain) article.append(element("p", `粮仓库存 ${measure("food", +grain.after)}`));
    if (harvest) article.append(element("p", `农田 ${measure("food", +harvest.before)}/班 · 渔点 ${measure("food", +harvest.after)}/班`));
    article.append(element("p", resolutionEffect(vote)));
    const log = element("button", `↖ #${vote.id} 表决记录`);
    log.type = "button";
    log.className = "cause";
    log.addEventListener("click", () => {
      setDecisionOpen(false);
      allEvents.checked = true;
      showDay(vote.day - 1, vote.id);
    });
    article.append(log);
    content.append(article);
  }
}

function showDay(index, targetId) {
  dayIndex = Math.max(0, Math.min(index, frames.length - 1));
  const frame = frames[dayIndex];
  for (const [location, plot] of plotsByDay[dayIndex]) {
    const button = tiles.get(location).button;
    button.classList.toggle("plot-wood", plot.accessible && plot.wood > 0);
    button.classList.toggle("plot-steel", plot.accessible && plot.steel > 0);
  }
  dayInput.value = frame.day;
  dayInput.style.setProperty("--progress", `${dayIndex / Math.max(1, frames.length - 1) * 100}%`);
  dayInput.setAttribute("aria-valuetext", `第 ${frame.day} 日`);
  byId("date").textContent = `第 ${frame.day} 日 / ${frames.length} 日`;
  renderCalendar(frame, targetId);
  byId("back").disabled = dayIndex === 0;
  byId("next").disabled = dayIndex === frames.length - 1;
  renderOverview(frame);
  const daily = eventsByDay.get(frame.day) ?? [];
  renderOrganizations(frame, daily);
  renderMapDecisions(frame, daily);
  renderGrid(frame, daily);
  renderWater(frame, daily);
  renderGranary(frame, daily);
  renderHealth(frame, daily);
  const pumpFailure = daily.find((event) => event.subject === "pump" && [
    "water_power_denied", "pump_out_of_service", "no_water_crew",
  ].includes(event.reason));
  const pump = tiles.get("5,5");
  pump.button.classList.toggle("interrupted", Boolean(pumpFailure));
  const status = pumpFailure ? ` · ${reasonNames[pumpFailure.reason]}` : "";
  pump.button.title = pump.label + status;
  pump.button.setAttribute("aria-label", pump.label + status);
  const clinic = tiles.get("8,4");
  clinic.button.classList.toggle("interrupted", frame.waitingForCare > 0);
  tiles.get("6,1").button.classList.toggle("interrupted", frame.gridRequested > gridOnline(frame));
  renderInspector(targetId);
}

function stop() {
  clearInterval(timer);
  timer = null;
  byId("play").textContent = "▶";
  byId("play").setAttribute("aria-label", "播放回放");
}

byId("play").disabled = frames.length < 2;
byId("play").addEventListener("click", () => {
  if (timer !== null) {
    stop();
    return;
  }
  if (dayIndex === frames.length - 1) showDay(0);
  byId("play").textContent = "❚❚";
  byId("play").setAttribute("aria-label", "暂停回放");
  timer = setInterval(() => {
    showDay(dayIndex + 1);
    if (dayIndex === frames.length - 1) stop();
  }, 280);
});
byId("back").addEventListener("click", () => { stop(); showDay(dayIndex - 1); });
byId("next").addEventListener("click", () => { stop(); showDay(dayIndex + 1); });
dayInput.addEventListener("input", () => { stop(); showDay(Number(dayInput.value) - 1); });
allEvents.addEventListener("change", () => renderInspector());
dossierClose.addEventListener("click", () => {
  dossierPanel.hidden = true;
  selected.button?.focus({ preventScroll: true });
});
function applyPhotoOrientation() {
  const image = byId("tile-photo-image");
  byId("tile-photo").classList.toggle("portrait", image.naturalHeight > image.naturalWidth);
}
byId("tile-photo-image").addEventListener("load", applyPhotoOrientation);
const photoDialog = byId("photo-dialog");
byId("tile-photo-image").addEventListener("click", () => {
  const source = byId("tile-photo-image");
  const large = byId("photo-dialog-image");
  large.src = source.src;
  large.alt = source.alt;
  if (typeof photoDialog.showModal === "function") photoDialog.showModal();
  else photoDialog.setAttribute("open", "");
});
byId("photo-dialog-close").addEventListener("click", () => photoDialog.close());
photoDialog.addEventListener("click", (event) => {
  if (event.target === photoDialog) photoDialog.close();
});
byId("water-open").addEventListener("click", () => select(tiles.get("5,5")));
byId("granary-open").addEventListener("click", () => select(tiles.get("6,4")));
byId("health-water").addEventListener("click", () => select(tiles.get("5,5")));
byId("health-grid").addEventListener("click", () => select(tiles.get("6,1")));

function installMapZoom() {
  const viewport = byId("map-viewport");
  const map = byId("map");
  const zoomIn = byId("zoom-in");
  const zoomOut = byId("zoom-out");
  const reset = byId("zoom-reset");
  const maxZoom = 3;
  const step = 1.15;
  const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  let zoom = 1;
  let targetZoom = 1;
  let zoomFrame;
  let previousFrame;
  let anchor = [0, 0];
  let directZoom = false;
  let nativePinch;
  let x = 0;
  let y = 0;
  let drag;
  let suppressClick = false;

  function render() {
    const limitX = Math.max(0, (map.offsetWidth * zoom - viewport.clientWidth) / 2);
    const limitY = Math.max(0, (map.offsetHeight * zoom - viewport.clientHeight) / 2);
    x = Math.max(-limitX, Math.min(limitX, x));
    y = Math.max(-limitY, Math.min(limitY, y));
    map.style.setProperty("--map-x", `${x}px`);
    map.style.setProperty("--map-y", `${y}px`);
    map.style.setProperty("--map-zoom", zoom);
    viewport.classList.toggle("zoomed", zoom > 1);
    zoomIn.disabled = zoom === maxZoom;
    zoomOut.disabled = zoom === 1;
    reset.textContent = `${Math.round(zoom * 100)}%`;
  }

  function setZoom(next, anchorX = 0, anchorY = 0) {
    const clamped = Math.max(1, Math.min(maxZoom, next));
    const ratio = clamped / zoom;
    x = anchorX - (anchorX - x) * ratio;
    y = anchorY - (anchorY - y) * ratio;
    zoom = clamped;
    render();
  }

  function animateZoom(now) {
    zoomFrame = undefined;
    const elapsed = Math.min(40, now - (previousFrame ?? (now - 16)));
    const weight = directZoom || reducedMotion ? 1 : 1 - Math.exp(-elapsed / 40);
    previousFrame = now;
    const next = zoom * Math.pow(targetZoom / zoom, weight);
    const settled = Math.abs(Math.log(targetZoom / next)) < .0003;
    setZoom(settled ? targetZoom : next, ...anchor);
    if (settled) previousFrame = undefined;
    else zoomFrame = requestAnimationFrame(animateZoom);
  }

  function queueZoom(next, point = [0, 0], direct = false) {
    targetZoom = Math.max(1, Math.min(maxZoom, next));
    anchor = point;
    directZoom = direct;
    if (zoomFrame === undefined) zoomFrame = requestAnimationFrame(animateZoom);
  }

  function interruptZoom() {
    if (zoomFrame !== undefined) cancelAnimationFrame(zoomFrame);
    zoomFrame = undefined;
    previousFrame = undefined;
    targetZoom = zoom;
  }

  function gestureAnchor(event) {
    const bounds = viewport.getBoundingClientRect();
    if (!Number.isFinite(event.clientX) || !Number.isFinite(event.clientY)) return [0, 0];
    return [event.clientX - bounds.left - bounds.width / 2, event.clientY - bounds.top - bounds.height / 2];
  }

  zoomIn.addEventListener("click", () => queueZoom(targetZoom * step));
  zoomOut.addEventListener("click", () => queueZoom(targetZoom / step));
  reset.addEventListener("click", () => queueZoom(1));
  viewport.addEventListener("wheel", (event) => {
    if (event.target.closest(".dossier, .decision-panel")) return;
    const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.clientHeight : 1;
    if (!event.ctrlKey && !event.metaKey) {
      if (zoom === 1) return;
      event.preventDefault();
      interruptZoom();
      x -= event.deltaX * unit;
      y -= event.deltaY * unit;
      render();
      return;
    }
    event.preventDefault();
    if (nativePinch !== undefined) return;
    const pixels = event.deltaY * unit;
    const pinch = event.ctrlKey && !event.metaKey && event.deltaMode === 0;
    const logChange = Math.max(-.14, Math.min(.14, -pixels * (pinch ? .008 : .0015)));
    queueZoom(targetZoom * Math.exp(logChange), gestureAnchor(event), pinch);
  }, { passive: false });

  // Desktop Safari exposes a scale directly for trackpad pinch. Chromium
  // and Firefox represent the gesture as pixel wheel events with ctrlKey.
  viewport.addEventListener("gesturestart", (event) => {
    event.preventDefault();
    interruptZoom();
    nativePinch = zoom;
  }, { passive: false });
  viewport.addEventListener("gesturechange", (event) => {
    if (nativePinch === undefined || !Number.isFinite(event.scale)) return;
    event.preventDefault();
    queueZoom(nativePinch * event.scale, gestureAnchor(event), true);
  }, { passive: false });
  viewport.addEventListener("gestureend", (event) => {
    if (nativePinch === undefined) return;
    event.preventDefault();
    nativePinch = undefined;
  }, { passive: false });

  viewport.addEventListener("pointerdown", (event) => {
    suppressClick = false;
    if (zoom === 1 || !event.isPrimary || event.button !== 0 || event.target.closest(".map-zoom, .dossier, .decision-panel, .decision-marker")) return;
    interruptZoom();
    drag = { id: event.pointerId, startX: event.clientX, startY: event.clientY, x, y };
  });
  viewport.addEventListener("pointermove", (event) => {
    if (drag?.id !== event.pointerId) return;
    const dx = event.clientX - drag.startX;
    const dy = event.clientY - drag.startY;
    if (!suppressClick) {
      if (Math.hypot(dx, dy) < 4) return;
      suppressClick = true;
      viewport.setPointerCapture(event.pointerId);
      viewport.classList.add("panning");
    }
    x = drag.x + dx;
    y = drag.y + dy;
    render();
  });
  function endDrag(event) {
    if (drag?.id !== event.pointerId) return;
    drag = undefined;
    viewport.classList.remove("panning");
  }
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
    viewport.addEventListener(type, endDrag);
  }
  viewport.addEventListener("click", (event) => {
    if (!suppressClick) return;
    suppressClick = false;
    if (event.detail === 0) return;
    event.stopPropagation();
    event.preventDefault();
  }, true);
  viewport.addEventListener("focusin", (event) => {
    if (zoom === 1 || !event.target.matches(".hex, .decision-marker")) return;
    const bounds = viewport.getBoundingClientRect();
    const tile = event.target.getBoundingClientRect();
    x += Math.max(0, bounds.left - tile.left) - Math.max(0, tile.right - bounds.right);
    y += Math.max(0, bounds.top - tile.top) - Math.max(0, tile.bottom - bounds.bottom);
    render();
  });
  function resize() {
    drawOuterGrid(map, viewport);
    render();
  }
  new ResizeObserver(resize).observe(viewport);
  resize();
}

function installDossierResize() {
  const dossier = byId("tile-dossier");
  const handle = byId("dossier-resize");
  const viewport = byId("map-viewport");
  const clampSize = (width, height) => [
    Math.max(220, Math.min(Math.max(260, viewport.clientWidth - 20), width)),
    Math.max(140, Math.min(Math.max(200, viewport.clientHeight - 20), height)),
  ];
  const applySize = (width, height) => {
    const [clampedWidth, clampedHeight] = clampSize(width, height);
    dossier.style.width = `${clampedWidth}px`;
    dossier.style.height = `${clampedHeight}px`;
    dossier.style.maxHeight = "none";
  };
  let drag;
  handle.addEventListener("pointerdown", (event) => {
    if (!event.isPrimary || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // Synthetic pointers and some embedded browsers cannot be captured.
    }
    const bounds = dossier.getBoundingClientRect();
    drag = { id: event.pointerId, x: event.clientX, y: event.clientY, width: bounds.width, height: bounds.height };
    dossier.classList.add("resizing");
  });
  handle.addEventListener("pointermove", (event) => {
    if (drag?.id !== event.pointerId) return;
    applySize(drag.width + event.clientX - drag.x, drag.height - (event.clientY - drag.y));
  });
  const end = (event) => {
    if (drag?.id !== event.pointerId) return;
    drag = undefined;
    dossier.classList.remove("resizing");
  };
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
    handle.addEventListener(type, end);
  }
  handle.addEventListener("dblclick", () => {
    dossier.style.width = "";
    dossier.style.height = "";
    dossier.style.maxHeight = "";
  });
  handle.addEventListener("keydown", (event) => {
    const step = event.shiftKey ? 48 : 16;
    const delta = { ArrowRight: [step, 0], ArrowLeft: [-step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] }[event.key];
    if (!delta) return;
    event.preventDefault();
    applySize(dossier.offsetWidth + delta[0], dossier.offsetHeight + delta[1]);
  });
}

installMapZoom();
installDossierResize();
renderSelection();
document.fonts?.ready.then(renderSelection);
showDay(0);
