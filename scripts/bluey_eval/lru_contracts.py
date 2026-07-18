"""Static, non-executing validation for returned LRU code artifacts."""

from __future__ import annotations

import ast
import re
from typing import Dict, Iterable, List, Optional, Sequence, Tuple


def python_source_from_code_artifact(body: str) -> str:
    """Extract Python/untagged fenced blocks without executing them."""
    normalized = body.replace("\r\n", "\n").replace("\r", "\n").strip()
    # Bluey's persisted code artifact is a sectioned workbench document. Isolate
    # CODE before scanning fences so an example inside NOTES cannot replace it.
    sectioned = re.match(r"(?is)^CODE\s*\n-+\s*\n(.*)$", normalized)
    if sectioned:
        normalized = re.split(
            r"(?m)^\s*(?:LINE NOTES|COMPLEXITY|NOTES)\s*\n-+\s*$",
            sectioned.group(1),
            maxsplit=1,
        )[0].strip()
    blocks = re.findall(
        r"```[ \t]*([^\n`]*)\n(.*?)```",
        normalized,
        re.S,
    )
    candidates = [
        code.strip()
        for language, code in blocks
        if language.strip().casefold() in ("", "py", "python", "python3")
        and code.strip()
    ]
    if candidates:
        return "\n\n".join(candidates)
    return normalized


def _attribute_tokens(node: ast.AST) -> set[str]:
    tokens: set[str] = set()
    for candidate in ast.walk(node):
        if isinstance(candidate, ast.Attribute):
            tokens.add(candidate.attr.casefold())
        elif isinstance(candidate, ast.Name):
            tokens.add(candidate.id.casefold())
    return tokens


def _reachable_class_methods(
    start: ast.AST,
    methods: Dict[str, ast.AST],
) -> List[ast.AST]:
    """Follow self.method() calls so helpers count only when the answer uses them."""
    reachable: List[ast.AST] = []
    pending = [start]
    visited: set[str] = set()
    while pending:
        node = pending.pop()
        name = getattr(node, "name", "")
        if name in visited:
            continue
        visited.add(name)
        reachable.append(node)
        for candidate in ast.walk(node):
            if not isinstance(candidate, ast.Call) or not isinstance(
                candidate.func, ast.Attribute
            ):
                continue
            owner = candidate.func.value
            if isinstance(owner, ast.Name) and owner.id == "self":
                called = methods.get(candidate.func.attr)
                if called is not None and candidate.func.attr not in visited:
                    pending.append(called)
    return reachable


def _mutated_link_attributes(nodes: Sequence[ast.AST]) -> set[str]:
    mutated: set[str] = set()

    def collect(target: ast.AST) -> None:
        if isinstance(target, ast.Attribute):
            if target.attr.casefold() in {"prev", "next", "head", "tail"}:
                mutated.add(target.attr.casefold())
        elif isinstance(target, (ast.Tuple, ast.List)):
            for item in target.elts:
                collect(item)

    for node in nodes:
        for candidate in ast.walk(node):
            if isinstance(candidate, (ast.Assign, ast.AnnAssign, ast.AugAssign)):
                targets = (
                    candidate.targets
                    if isinstance(candidate, ast.Assign)
                    else [candidate.target]
                )
                for target in targets:
                    collect(target)
    return mutated


def _initialized_lock_attributes(class_node: ast.ClassDef) -> set[str]:
    initialized: set[str] = set()

    def is_lock_call(value: ast.AST) -> bool:
        if not isinstance(value, ast.Call):
            return False
        function = value.func
        if isinstance(function, ast.Attribute):
            name = function.attr
        elif isinstance(function, ast.Name):
            name = function.id
        else:
            return False
        return name.casefold() in {"lock", "rlock"}

    for candidate in ast.walk(class_node):
        if isinstance(candidate, ast.Assign):
            targets = candidate.targets
            value = candidate.value
        elif isinstance(candidate, ast.AnnAssign):
            targets = [candidate.target]
            value = candidate.value
        else:
            continue
        if value is None or not is_lock_call(value):
            continue
        for target in targets:
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "self"
            ):
                initialized.add(target.attr)
            elif isinstance(target, ast.Name):
                initialized.add(target.id)
    return initialized


def _constructor_self_assignment_attributes(class_node: ast.ClassDef) -> set[str]:
    """Find constructor assignments that read the same uninitialized attribute.

    This is a static, non-executing check for a common generated-code defect such
    as ``self.value = self.value``. It deliberately applies only inside
    ``__init__`` and only when both sides are the identical ``self`` attribute,
    so ordinary state updates elsewhere are unaffected.
    """
    # A base class or class-level descriptor/default can legitimately provide
    # the right-hand attribute. The evaluator cannot prove those values absent,
    # so it fails safe instead of rejecting the artifact.
    if class_node.bases:
        return set()

    initialized: set[str] = set()
    for member in class_node.body:
        if isinstance(member, ast.Assign):
            for target in member.targets:
                if isinstance(target, ast.Name):
                    initialized.add(target.id)
        elif isinstance(member, ast.AnnAssign) and isinstance(member.target, ast.Name):
            initialized.add(member.target.id)
        elif isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef)):
            initialized.add(member.name)

    initializer = next(
        (
            node
            for node in class_node.body
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
            and node.name == "__init__"
        ),
        None,
    )
    if initializer is None:
        return set()

    suspicious: set[str] = set()
    assignments = sorted(
        (
            candidate
            for candidate in ast.walk(initializer)
            if isinstance(candidate, (ast.Assign, ast.AnnAssign))
        ),
        key=lambda node: (getattr(node, "lineno", 0), getattr(node, "col_offset", 0)),
    )
    for candidate in assignments:
        if not isinstance(candidate, (ast.Assign, ast.AnnAssign)):
            continue
        value = candidate.value
        targets = candidate.targets if isinstance(candidate, ast.Assign) else [candidate.target]
        if (
            isinstance(value, ast.Attribute)
            and isinstance(value.value, ast.Name)
            and value.value.id == "self"
            and value.attr not in initialized
        ):
            for target in targets:
                if (
                    isinstance(target, ast.Attribute)
                    and isinstance(target.value, ast.Name)
                    and target.value.id == "self"
                    and target.attr == value.attr
                ):
                    suspicious.add(target.attr)
        for target in targets:
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "self"
            ):
                initialized.add(target.attr)
    return suspicious


def _lru_implementation_classes(
    tree: ast.AST,
    lru_class: ast.ClassDef,
) -> List[ast.ClassDef]:
    """Return the LRU class plus packaged helper classes it constructs."""
    classes = {
        candidate.name: candidate
        for candidate in ast.walk(tree)
        if isinstance(candidate, ast.ClassDef)
    }
    relevant: List[ast.ClassDef] = []
    pending = [lru_class]
    visited: set[str] = set()
    while pending:
        class_node = pending.pop()
        if class_node.name in visited:
            continue
        visited.add(class_node.name)
        relevant.append(class_node)
        for candidate in ast.walk(class_node):
            if not isinstance(candidate, ast.Call) or not isinstance(
                candidate.func, ast.Name
            ):
                continue
            helper = classes.get(candidate.func.id)
            if helper is not None and helper.name not in visited:
                pending.append(helper)
    return relevant


def _assignment_target_value_pairs(
    target: ast.AST, value: ast.AST
) -> Iterable[Tuple[ast.AST, ast.AST]]:
    if (
        isinstance(target, (ast.Tuple, ast.List))
        and isinstance(value, (ast.Tuple, ast.List))
        and len(target.elts) == len(value.elts)
    ):
        for nested_target, nested_value in zip(target.elts, value.elts):
            yield from _assignment_target_value_pairs(nested_target, nested_value)
        return
    yield target, value


def _direct_self_attribute(node: ast.AST) -> Optional[str]:
    if (
        isinstance(node, ast.Attribute)
        and isinstance(node.value, ast.Name)
        and node.value.id == "self"
    ):
        return node.attr
    return None


def _cross_linked_lru_sentinel_pair(
    lru_class: ast.ClassDef,
) -> Optional[Tuple[str, str]]:
    initializer = next(
        (
            node
            for node in lru_class.body
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
            and node.name == "__init__"
        ),
        None,
    )
    if initializer is None:
        return None

    initialized: set[str] = set()
    links: set[Tuple[str, str, str]] = set()
    for candidate in ast.walk(initializer):
        if isinstance(candidate, ast.Assign):
            assignments = [
                pair
                for target in candidate.targets
                for pair in _assignment_target_value_pairs(target, candidate.value)
            ]
        elif isinstance(candidate, ast.AnnAssign) and candidate.value is not None:
            assignments = list(
                _assignment_target_value_pairs(candidate.target, candidate.value)
            )
        else:
            continue

        for target, value in assignments:
            if direct := _direct_self_attribute(target):
                initialized.add(direct)
                continue
            if (
                isinstance(target, ast.Attribute)
                and target.attr in {"next", "prev"}
                and isinstance(target.value, ast.Attribute)
            ):
                boundary = _direct_self_attribute(target.value)
                peer = _direct_self_attribute(value)
                if boundary is not None and peer is not None:
                    links.add((boundary, target.attr, peer))

    for first, second in (("head", "tail"), ("left", "right")):
        if not {first, second}.issubset(initialized):
            continue
        forward = (first, "next", second) in links and (second, "prev", first) in links
        reverse = (second, "next", first) in links and (first, "prev", second) in links
        if forward or reverse:
            return (first, second)
    return None


def _scope_references_sentinel(
    nodes: Sequence[ast.AST], sentinel_pair: Tuple[str, str]
) -> bool:
    sentinels = set(sentinel_pair)
    return any(
        _direct_self_attribute(candidate) in sentinels
        for node in nodes
        for candidate in ast.walk(node)
    )


def _scope_references_sentinel_neighbor(
    nodes: Sequence[ast.AST], sentinel_pair: Tuple[str, str]
) -> bool:
    sentinels = set(sentinel_pair)
    return any(
        isinstance(candidate, ast.Attribute)
        and candidate.attr in {"next", "prev"}
        and isinstance(candidate.value, ast.Attribute)
        and _direct_self_attribute(candidate.value) in sentinels
        for node in nodes
        for candidate in ast.walk(node)
    )


def _references_sentinel(node: ast.AST, sentinel_pair: Tuple[str, str]) -> bool:
    sentinels = set(sentinel_pair)
    return any(
        _direct_self_attribute(candidate) in sentinels
        for candidate in ast.walk(node)
    )


def _scope_relinks_through_sentinel(
    nodes: Sequence[ast.AST], sentinel_pair: Tuple[str, str]
) -> bool:
    """Require one node to be reciprocally attached beside a sentinel."""

    def aliases_for_boundary(scope: ast.AST) -> set[str]:
        aliases: set[str] = set()
        assignments: List[Tuple[ast.AST, ast.AST]] = []
        for candidate in ast.walk(scope):
            if isinstance(candidate, ast.Assign):
                assignments.extend(
                    pair
                    for target in candidate.targets
                    for pair in _assignment_target_value_pairs(target, candidate.value)
                )
            elif isinstance(candidate, ast.AnnAssign) and candidate.value is not None:
                assignments.extend(
                    _assignment_target_value_pairs(candidate.target, candidate.value)
                )
        changed = True
        while changed:
            changed = False
            for target, value in assignments:
                if not isinstance(target, ast.Name) or target.id in aliases:
                    continue
                if _references_sentinel(value, sentinel_pair) or any(
                    isinstance(nested, ast.Name) and nested.id in aliases
                    for nested in ast.walk(value)
                ):
                    aliases.add(target.id)
                    changed = True
        return aliases

    def references_boundary_or_alias(node: ast.AST, aliases: set[str]) -> bool:
        return _references_sentinel(node, sentinel_pair) or any(
            isinstance(candidate, ast.Name) and candidate.id in aliases
            for candidate in ast.walk(node)
        )

    for scope in nodes:
        aliases = aliases_for_boundary(scope)
        node_links: Dict[str, set[str]] = {}
        reciprocal_links: Dict[str, set[str]] = {}
        for candidate in ast.walk(scope):
            if isinstance(candidate, ast.Assign):
                assignments = [
                    pair
                    for target in candidate.targets
                    for pair in _assignment_target_value_pairs(target, candidate.value)
                ]
            elif isinstance(candidate, ast.AnnAssign) and candidate.value is not None:
                assignments = list(
                    _assignment_target_value_pairs(candidate.target, candidate.value)
                )
            else:
                continue
            for target, value in assignments:
                if not (
                    isinstance(target, ast.Attribute)
                    and target.attr in {"next", "prev"}
                ):
                    continue
                if isinstance(target.value, ast.Name) and references_boundary_or_alias(
                    value, aliases
                ):
                    node_links.setdefault(target.value.id, set()).add(target.attr)
                if references_boundary_or_alias(target, aliases) and isinstance(
                    value, ast.Name
                ):
                    reciprocal_links.setdefault(value.id, set()).add(target.attr)
        if any(
            {"prev", "next"}.issubset(link_kinds)
            and {"prev", "next"}.issubset(reciprocal_links.get(node_name, set()))
            for node_name, link_kinds in node_links.items()
        ):
            return True
    return False


def _sentinel_neighbor(node: ast.AST, sentinel_pair: Tuple[str, str]) -> bool:
    return (
        isinstance(node, ast.Attribute)
        and node.attr in {"next", "prev"}
        and isinstance(node.value, ast.Attribute)
        and _direct_self_attribute(node.value) in set(sentinel_pair)
    )


def _bound_sentinel_neighbors(
    nodes: Sequence[ast.AST], sentinel_pair: Tuple[str, str]
) -> set[str]:
    bound: set[str] = set()
    for node in nodes:
        for candidate in ast.walk(node):
            if isinstance(candidate, ast.Assign):
                assignments = [
                    pair
                    for target in candidate.targets
                    for pair in _assignment_target_value_pairs(target, candidate.value)
                ]
            elif isinstance(candidate, ast.AnnAssign) and candidate.value is not None:
                assignments = list(
                    _assignment_target_value_pairs(candidate.target, candidate.value)
                )
            else:
                continue
            for target, value in assignments:
                if isinstance(target, ast.Name) and _sentinel_neighbor(
                    value, sentinel_pair
                ):
                    bound.add(target.id)
    return bound


def _references_bound_node_key(node: ast.AST, bound_name: str) -> bool:
    return any(
        isinstance(candidate, ast.Attribute)
        and candidate.attr == "key"
        and isinstance(candidate.value, ast.Name)
        and candidate.value.id == bound_name
        for candidate in ast.walk(node)
    )


def _scope_deletes_bound_node_from_map(
    nodes: Sequence[ast.AST], bound_name: str
) -> bool:
    for node in nodes:
        for candidate in ast.walk(node):
            if isinstance(candidate, ast.Delete):
                if any(
                    isinstance(target, ast.Subscript)
                    and _references_bound_node_key(target.slice, bound_name)
                    for target in candidate.targets
                ):
                    return True
            if not (
                isinstance(candidate, ast.Call)
                and isinstance(candidate.func, ast.Attribute)
                and candidate.func.attr.casefold() in {"pop", "__delitem__"}
                and candidate.args
            ):
                continue
            if _references_bound_node_key(candidate.args[0], bound_name):
                return True
    return False


def _scope_unlinks_bound_node(
    nodes: Sequence[ast.AST], bound_name: str, methods: Dict[str, ast.AST]
) -> bool:
    def reciprocally_unlinks(scope: ast.AST, node_name: str) -> bool:
        aliases: Dict[str, str] = {}
        assignments: List[Tuple[ast.AST, ast.AST]] = []
        for candidate in ast.walk(scope):
            if isinstance(candidate, ast.Assign):
                assignments.extend(
                    pair
                    for target in candidate.targets
                    for pair in _assignment_target_value_pairs(target, candidate.value)
                )
            elif isinstance(candidate, ast.AnnAssign) and candidate.value is not None:
                assignments.extend(
                    _assignment_target_value_pairs(candidate.target, candidate.value)
                )

        def neighbor_kind(expression: ast.AST) -> Optional[str]:
            if isinstance(expression, ast.Name):
                return aliases.get(expression.id)
            if (
                isinstance(expression, ast.Attribute)
                and expression.attr in {"prev", "next"}
                and isinstance(expression.value, ast.Name)
                and expression.value.id == node_name
            ):
                return expression.attr
            return None

        changed = True
        while changed:
            changed = False
            for target, value in assignments:
                if not isinstance(target, ast.Name) or target.id in aliases:
                    continue
                kind = neighbor_kind(value)
                if kind is not None:
                    aliases[target.id] = kind
                    changed = True

        reconnects_previous = False
        reconnects_next = False
        for target, value in assignments:
            if not isinstance(target, ast.Attribute):
                continue
            owner_kind = neighbor_kind(target.value)
            value_kind = neighbor_kind(value)
            reconnects_previous = reconnects_previous or (
                owner_kind == "prev" and target.attr == "next" and value_kind == "next"
            )
            reconnects_next = reconnects_next or (
                owner_kind == "next" and target.attr == "prev" and value_kind == "prev"
            )
        return reconnects_previous and reconnects_next

    if any(reciprocally_unlinks(node, bound_name) for node in nodes):
        return True
    for node in nodes:
        for candidate in ast.walk(node):
            if not (
                isinstance(candidate, ast.Call)
                and isinstance(candidate.func, ast.Attribute)
                and isinstance(candidate.func.value, ast.Name)
                and candidate.func.value.id == "self"
                and any(
                    isinstance(argument, ast.Name) and argument.id == bound_name
                    for argument in candidate.args
                )
            ):
                continue
            helper = methods.get(candidate.func.attr)
            if helper is None:
                continue
            parameters = [
                argument.arg
                for argument in [*helper.args.posonlyargs, *helper.args.args]
                if argument.arg != "self"
            ]
            for index, argument in enumerate(candidate.args):
                if (
                    isinstance(argument, ast.Name)
                    and argument.id == bound_name
                    and index < len(parameters)
                    and reciprocally_unlinks(helper, parameters[index])
                ):
                    return True
    return False


def _capacity_guard_evicts_bound_sentinel_neighbor(
    guard: ast.AST,
    methods: Dict[str, ast.AST],
    sentinel_pair: Tuple[str, str],
) -> bool:
    scopes = _reachable_class_methods(guard, methods)
    for bound_name in _bound_sentinel_neighbors(scopes, sentinel_pair):
        if _scope_deletes_bound_node_from_map(
            scopes, bound_name
        ) and _scope_unlinks_bound_node(scopes, bound_name, methods):
            return True
    return False


def _has_nonpositive_capacity_exit(lru_class: ast.ClassDef) -> bool:
    """Recognize an implemented zero-capacity return/rejection without execution."""

    def is_capacity(expression: ast.AST) -> bool:
        return (
            isinstance(expression, ast.Name) and expression.id == "capacity"
        ) or (
            isinstance(expression, ast.Attribute)
            and expression.attr == "capacity"
            and isinstance(expression.value, ast.Name)
            and expression.value.id == "self"
        )

    def integer(expression: ast.AST) -> Optional[int]:
        if isinstance(expression, ast.Constant) and type(expression.value) is int:
            return expression.value
        return None

    def tests_nonpositive(expression: ast.AST) -> bool:
        if isinstance(expression, ast.UnaryOp) and isinstance(expression.op, ast.Not):
            return is_capacity(expression.operand)
        if not (
            isinstance(expression, ast.Compare)
            and len(expression.ops) == 1
            and len(expression.comparators) == 1
        ):
            return False
        left, operator, right = (
            expression.left,
            expression.ops[0],
            expression.comparators[0],
        )
        right_value = integer(right)
        if is_capacity(left) and right_value is not None:
            return (
                isinstance(operator, ast.Eq) and right_value == 0
            ) or (
                isinstance(operator, ast.LtE) and right_value == 0
            ) or (
                isinstance(operator, ast.Lt) and right_value == 1
            )
        left_value = integer(left)
        if left_value is not None and is_capacity(right):
            return (
                isinstance(operator, ast.Eq) and left_value == 0
            ) or (
                isinstance(operator, ast.GtE) and left_value == 0
            ) or (
                isinstance(operator, ast.Gt) and left_value == 1
            )
        return False

    for method_name in ("__init__", "put"):
        method = next(
            (
                member
                for member in lru_class.body
                if isinstance(member, (ast.FunctionDef, ast.AsyncFunctionDef))
                and member.name == method_name
            ),
            None,
        )
        if method is None:
            continue
        for candidate in ast.walk(method):
            if not isinstance(candidate, ast.If) or not tests_nonpositive(candidate.test):
                continue
            if any(
                isinstance(statement, (ast.Return, ast.Raise))
                for body_statement in candidate.body
                for statement in ast.walk(body_statement)
            ):
                return True
    return False


def _used_lock_attributes(nodes: Sequence[ast.AST]) -> set[str]:
    used: set[str] = set()
    for node in nodes:
        for candidate in ast.walk(node):
            if isinstance(candidate, (ast.With, ast.AsyncWith)):
                for item in candidate.items:
                    expression = item.context_expr
                    if (
                        isinstance(expression, ast.Attribute)
                        and isinstance(expression.value, ast.Name)
                        and expression.value.id == "self"
                    ):
                        used.add(expression.attr)
            if not isinstance(candidate, ast.Call) or not isinstance(
                candidate.func, ast.Attribute
            ):
                continue
            if candidate.func.attr not in {"acquire", "release"}:
                continue
            lock = candidate.func.value
            if (
                isinstance(lock, ast.Attribute)
                and isinstance(lock.value, ast.Name)
                and lock.value.id == "self"
            ):
                used.add(lock.attr)
    return used


def lru_code_semantic_issues(case_id: str, body: str) -> List[str]:
    """Validate runnable LRU behavior and Q09's lock in the returned Python AST."""
    source = python_source_from_code_artifact(body)
    try:
        tree = ast.parse(source)
        compile(tree, "<bluey-code-artifact>", "exec")
    except (SyntaxError, ValueError, TypeError, MemoryError):
        return ["invalid_python_code_artifact"]

    lru_class = next(
        (
            node
            for node in ast.walk(tree)
            if isinstance(node, ast.ClassDef)
            and re.sub(r"[^a-z0-9]", "", node.name.casefold()) == "lrucache"
        ),
        None,
    )
    if lru_class is None:
        return ["missing_lru_cache_class"]

    method_types = (ast.FunctionDef, ast.AsyncFunctionDef)
    methods: Dict[str, ast.AST] = {
        node.name: node for node in lru_class.body if isinstance(node, method_types)
    }
    get_method = methods.get("get")
    put_method = methods.get("put")
    if get_method is None or put_method is None:
        return ["missing_lru_get_or_put_implementation"]

    issues: List[str] = []
    suspicious_constructor_assignments = set()
    for candidate in _lru_implementation_classes(tree, lru_class):
        suspicious_constructor_assignments.update(
            _constructor_self_assignment_attributes(candidate)
        )
    if suspicious_constructor_assignments:
        issues.append("suspicious_constructor_self_assignment")
    class_tokens = _attribute_tokens(tree)
    sentinel_pair = _cross_linked_lru_sentinel_pair(lru_class)
    has_linked_recency = (
        {"prev", "next"}.issubset(class_tokens)
        and sentinel_pair is not None
        and not bool({"ordereddict", "functools.lru_cache"} & class_tokens)
    )
    if not has_linked_recency:
        issues.append("missing_lru_linked_recency_structure")
    if not _has_nonpositive_capacity_exit(lru_class):
        issues.append("missing_lru_zero_capacity_guard")

    get_scope = _reachable_class_methods(get_method, methods)
    put_scope = _reachable_class_methods(put_method, methods)
    get_mutations = _mutated_link_attributes(get_scope)
    if (
        not {"prev", "next"}.issubset(get_mutations)
        or sentinel_pair is None
        or not _scope_relinks_through_sentinel(get_scope, sentinel_pair)
    ):
        issues.append("missing_lru_recency_update_in_get")

    capacity_guards = [
        candidate
        for node in put_scope
        for candidate in ast.walk(node)
        if isinstance(candidate, (ast.If, ast.While))
        and "capacity" in _attribute_tokens(candidate.test)
        and bool(
            {"len", "size", "cache", "nodes", "map"}
            & _attribute_tokens(candidate.test)
        )
    ]
    has_capacity_guard = bool(capacity_guards)
    has_destructive_eviction = any(
        isinstance(candidate, ast.Delete)
        or (
            isinstance(candidate, ast.Call)
            and isinstance(candidate.func, ast.Attribute)
            and candidate.func.attr.casefold() in {"pop", "popitem"}
        )
        or (
            isinstance(candidate, ast.Call)
            and isinstance(candidate.func, ast.Attribute)
            and isinstance(candidate.func.value, ast.Name)
            and candidate.func.value.id == "self"
            and "evict" in candidate.func.attr.casefold()
        )
        for node in put_scope
        for candidate in ast.walk(node)
    )
    has_lru_eviction_target = sentinel_pair is not None and any(
        _capacity_guard_evicts_bound_sentinel_neighbor(guard, methods, sentinel_pair)
        for guard in capacity_guards
    )
    if not (has_capacity_guard and has_destructive_eviction and has_lru_eviction_target):
        issues.append("missing_lru_capacity_eviction")

    if case_id == "Q09":
        initialized_locks = _initialized_lock_attributes(lru_class)
        get_locks = _used_lock_attributes(get_scope) & initialized_locks
        put_locks = _used_lock_attributes(put_scope) & initialized_locks
        if not initialized_locks:
            issues.append("missing_shared_lock_initialization")
        if not get_locks:
            issues.append("missing_lock_usage_in_get")
        if not put_locks:
            issues.append("missing_lock_usage_in_put")
        if get_locks and put_locks and not (get_locks & put_locks):
            issues.append("get_and_put_use_different_locks")
    return issues
