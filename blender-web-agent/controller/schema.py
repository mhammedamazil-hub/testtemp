"""Host-side validation for bw.agent.ops/v2.

This mirrors the executor's OP_SPEC (harness/bw_ops.py) so a malformed plan is
rejected before paying for a ~15 s runtime boot. `api.describe` inside the
runtime is the authority; tests/test_controller.py checks the two agree.
"""
from __future__ import annotations

SCHEMA = "bw.agent.ops/v2"

# op -> required fields
OP_SPEC: dict[str, list[str]] = {
    "scene.reset": [],
    "scene.inspect": [],
    "scene.snapshot": [],
    "scene.restore": ["snapshot"],
    "object.create": ["type", "name"],
    "object.set_dimensions": ["target", "dimensions"],
    "object.transform": ["target"],
    "object.delete": ["target"],
    "selection.set": ["targets"],
    "mesh.extrude": ["target", "distance"],
    "mesh.bevel": ["target", "width"],
    "mesh.subdivide": ["target"],
    "material.create": ["name", "base_color"],
    "material.assign": ["material", "target"],
    "camera.create": ["name"],
    "light.create": ["type"],
    "export": ["formats", "prefix"],
    "validate.roundtrip": ["formats", "prefix"],
    "api.describe": [],
}

EXPORT_FORMATS = ("glb", "gltf", "obj", "stl", "ply", "usda", "blend")
UNSUPPORTED_FORMATS = {"fbx": "io_scene_fbx is compiled out of this build"}

PRIMITIVES = ("cube", "sphere", "cylinder", "cone", "plane", "torus")
LIGHT_TYPES = ("SUN", "POINT", "SPOT", "AREA")

DEFAULT_LIMITS = {"max_ops": 120, "deadline_s": 240.0}


class PlanError(ValueError):
    def __init__(self, code: str, message: str, where: str | None = None):
        super().__init__(message)
        self.code = code
        self.message = message
        self.where = where

    def to_dict(self) -> dict:
        out = {"code": self.code, "message": self.message}
        if self.where:
            out["where"] = self.where
        return out


def _check_op(op: dict, where: str) -> None:
    name = op.get("op")
    if name not in OP_SPEC:
        raise PlanError("UNKNOWN_OP", "unknown op %r (fail closed)" % (name,), where)
    missing = [k for k in OP_SPEC[name] if k not in op]
    if missing:
        raise PlanError("SCHEMA_INVALID", "%s missing %s" % (name, missing), where)

    if name == "object.create":
        if op["type"] not in PRIMITIVES:
            raise PlanError("UNSUPPORTED_PRIMITIVE",
                            "type must be one of %s" % (PRIMITIVES,), where)
        if not isinstance(op["name"], str) or not op["name"].strip():
            raise PlanError("SCHEMA_INVALID", "name must be a non-empty string", where)
    elif name == "light.create":
        if op["type"] not in LIGHT_TYPES:
            raise PlanError("SCHEMA_INVALID", "light type must be one of %s" % (LIGHT_TYPES,), where)
    elif name == "material.create":
        c = op["base_color"]
        if not (isinstance(c, list) and len(c) == 3
                and all(isinstance(x, (int, float)) and 0.0 <= x <= 1.0 for x in c)):
            raise PlanError("SCHEMA_INVALID",
                            "base_color must be 3 floats in 0..1", where)
    elif name == "object.set_dimensions":
        d = op["dimensions"]
        if not (isinstance(d, list) and len(d) == 3
                and all(isinstance(x, (int, float)) and x > 0 for x in d)):
            raise PlanError("SCHEMA_INVALID", "dimensions must be 3 positive numbers", where)
    elif name in ("export", "validate.roundtrip"):
        fmts = op["formats"]
        if not isinstance(fmts, list) or not fmts:
            raise PlanError("SCHEMA_INVALID", "formats must be a non-empty list", where)
        for f in fmts:
            if f in UNSUPPORTED_FORMATS:
                raise PlanError("UNSUPPORTED_FORMAT",
                                "%s is unsupported: %s" % (f, UNSUPPORTED_FORMATS[f]), where)
            if f not in EXPORT_FORMATS:
                raise PlanError("UNSUPPORTED_FORMAT", "unknown format %r" % (f,), where)
        if not isinstance(op.get("prefix"), str) or "/" in op.get("prefix", ""):
            raise PlanError("SCHEMA_INVALID", "prefix must be a plain filename stem", where)
    elif name == "mesh.extrude":
        if float(op["distance"]) <= 0:
            raise PlanError("SCHEMA_INVALID", "distance must be > 0", where)
    elif name == "mesh.bevel":
        if float(op["width"]) <= 0:
            raise PlanError("SCHEMA_INVALID", "width must be > 0", where)
    elif name == "selection.set":
        if not isinstance(op["targets"], list):
            raise PlanError("SCHEMA_INVALID", "targets must be a list", where)


def validate_plan(plan: dict) -> None:
    """Raise PlanError on the first problem. Validates the whole plan up front."""
    if not isinstance(plan, dict):
        raise PlanError("SCHEMA_INVALID", "plan must be a JSON object")
    if plan.get("schema") != SCHEMA:
        raise PlanError("SCHEMA_INVALID", "schema must be %r" % SCHEMA)
    tasks = plan.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        raise PlanError("SCHEMA_INVALID", "plan needs a non-empty tasks array")

    limits = plan.get("limits", {}) or {}
    try:
        max_ops = int(limits.get("max_ops", DEFAULT_LIMITS["max_ops"]))
        float(limits.get("deadline_s", DEFAULT_LIMITS["deadline_s"]))
    except (TypeError, ValueError):
        raise PlanError("SCHEMA_INVALID", "limits must be numeric") from None
    if max_ops < 1:
        raise PlanError("SCHEMA_INVALID", "limits.max_ops must be >= 1")

    seen = set()
    total_ops = 0
    for ti, task in enumerate(tasks):
        where = "tasks[%d]" % ti
        if not isinstance(task, dict) or not task.get("id"):
            raise PlanError("SCHEMA_INVALID", "every task needs an id", where)
        if task["id"] in seen:
            raise PlanError("SCHEMA_INVALID", "duplicate task id %r" % task["id"], where)
        seen.add(task["id"])
        ops = task.get("ops")
        if not isinstance(ops, list) or not ops:
            raise PlanError("SCHEMA_INVALID", "task %r has no ops" % task["id"], where)
        for oi, op in enumerate(ops):
            if not isinstance(op, dict):
                raise PlanError("SCHEMA_INVALID", "op is not an object", "%s.ops[%d]" % (where, oi))
            _check_op(op, "%s.ops[%d]" % (where, oi))
        total_ops += len(ops)
    if total_ops > max_ops:
        raise PlanError("LIMIT_EXCEEDED",
                        "plan has %d ops, max_ops is %d" % (total_ops, max_ops))


def describe() -> dict:
    """Compact schema summary handed to the model."""
    return {
        "schema": SCHEMA,
        "ops": {name: {"required": req} for name, req in OP_SPEC.items()},
        "primitives": list(PRIMITIVES),
        "light_types": list(LIGHT_TYPES),
        "export_formats": list(EXPORT_FORMATS),
        "unsupported_formats": UNSUPPORTED_FORMATS,
        "target_forms": ["<name>", {"name": "<name>"}, {"bw_id": "o..."}],
        "notes": [
            "object.create fails with DUPLICATE_OBJECT if the name exists; "
            "pass if_exists=skip to reuse.",
            "mesh ops act on an explicit target; there is no implicit active object.",
            "Every op returns {ok, result|error, scene}; mutating ops include fresh scene state.",
            "export returns per-format ok/bytes/sha256 and streams small files back.",
        ],
    }
