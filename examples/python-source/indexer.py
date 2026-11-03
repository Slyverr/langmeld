#!/usr/bin/env python3
"""Persistent, isolated CPython AST worker for the Python source example."""
import ast
import io
import json
import sys
import tokenize

MAX_REQUEST_BYTES = 6 * 4 * 1024 * 1024 + 64 * 1024


def parse_source(source):
    lines = source.splitlines(keepends=True)
    line_starts = []
    cursor = 0
    for line in lines:
        line_starts.append(cursor)
        cursor += len(line.encode("utf-8"))
    if not lines:
        line_starts.append(0)

    def byte_offset(line, column):
        return line_starts[line - 1] + column

    def span(node):
        return [byte_offset(node.lineno, node.col_offset),
                byte_offset(node.end_lineno, node.end_col_offset)]

    def source_segment(node):
        text = ast.get_source_segment(source, node)
        return text if text is not None else ""

    def name_span(line, column, name):
        raw = lines[line - 1].encode("utf-8")
        start = raw.find(name.encode("utf-8"), column)
        if start < 0:
            return [byte_offset(line, column), byte_offset(line, column)]
        return [line_starts[line - 1] + start,
                line_starts[line - 1] + start + len(name.encode("utf-8"))]

    def alias_name_span(alias):
        raw = lines[alias.lineno - 1].encode("utf-8")
        start = alias.col_offset
        end = alias.end_col_offset
        token = alias.asname or alias.name
        segment = raw[start:end]
        offset = segment.rfind(token.encode("utf-8"))
        if offset < 0:
            return [line_starts[alias.lineno - 1] + start,
                    line_starts[alias.lineno - 1] + end]
        begin = line_starts[alias.lineno - 1] + start + offset
        return [begin, begin + len(token.encode("utf-8"))]

    def alias_imported_span(alias):
        raw = lines[alias.lineno - 1].encode("utf-8")
        start = alias.col_offset
        end = alias.end_col_offset
        token = alias.name.encode("utf-8")
        offset = raw[start:end].find(token)
        if offset < 0:
            return [line_starts[alias.lineno - 1] + start,
                    line_starts[alias.lineno - 1] + end]
        begin = line_starts[alias.lineno - 1] + start + offset
        return [begin, begin + len(token)]

    syntax_error = None
    try:
        module = ast.parse(source, filename="<stdin>", type_comments=True)
    except SyntaxError as error:
        line = max(1, error.lineno or 1)
        text = lines[line - 1] if line <= len(lines) else ""
        start_column = max(0, (error.offset or 1) - 1)
        start = line_starts[line - 1] + len(text[:start_column].encode("utf-8")) if line <= len(lines) else len(source.encode("utf-8"))
        end_column = max(start_column + 1, (error.end_offset or start_column + 2) - 1)
        end = line_starts[line - 1] + len(text[:end_column].encode("utf-8")) if line <= len(lines) else start
        syntax_error = {"message": error.msg, "span": [start, end]}
        recovered = []
        indent = 0
        active_start = None
        active_has_suite = False

        def finish_active(end_line):
            if active_start is None:
                return
            chunk = "".join(lines[active_start - 1:end_line])
            try:
                parsed = ast.parse(chunk, filename="<stdin>", type_comments=True)
                ast.increment_lineno(parsed, active_start - 1)
                recovered.extend(parsed.body)
            except SyntaxError:
                pass

        try:
            for token in tokenize.generate_tokens(io.StringIO(source).readline):
                if token.type == tokenize.INDENT:
                    indent += 1
                    if active_start is not None:
                        active_has_suite = True
                elif token.type == tokenize.DEDENT:
                    indent = max(0, indent - 1)
                    if active_start is not None and active_has_suite and indent == 0:
                        finish_active(token.start[0] - 1)
                        active_start = None
                        active_has_suite = False
                elif token.type == tokenize.NAME and token.string == "def" and indent == 0:
                    if active_start is not None:
                        finish_active(token.start[0] - 1)
                    start_line = token.start[0]
                    while start_line > 1 and lines[start_line - 2].lstrip().startswith("@"):
                        start_line -= 1
                    active_start = start_line
                    active_has_suite = False
        except (IndentationError, tokenize.TokenError):
            pass
        if active_start is not None:
            finish_active(len(lines))

        recovered_imports = []
        for line_index, text in enumerate(lines):
            if text.startswith("from ."):
                try:
                    statement = ast.parse(text, filename="<stdin>", type_comments=True)
                    ast.increment_lineno(statement, line_index)
                    recovered_imports.extend(statement.body)
                except SyntaxError:
                    pass
        module = ast.Module(body=recovered_imports + recovered, type_ignores=[])

    functions = [node for node in module.body
                 if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))]
    local_names = {node.name for node in functions}
    imports = []
    import_targets = {}
    for node in module.body:
        if isinstance(node, ast.ImportFrom) and node.level == 1 and node.module:
            names = []
            for alias in node.names:
                local_name = alias.asname or alias.name
                import_targets[local_name] = alias.name
                names.append({"imported_name": alias.name,
                              "local_name": local_name,
                              "span": span(alias),
                              "name_span": alias_imported_span(alias),
                              "local_name_span": alias_name_span(alias)})
            imports.append({"module": node.module,
                            "span": span(node),
                            "module_span": [byte_offset(node.lineno, node.col_offset + 6),
                                            byte_offset(node.lineno, node.col_offset + 6 + len(node.module))],
                            "names": names})
    call_targets = {name: name for name in local_names}
    call_targets.update(import_targets)

    class DirectCalls(ast.NodeVisitor):
        def __init__(self):
            self.calls = []

        def visit_Call(self, node):
            if isinstance(node.func, ast.Name) and node.func.id in call_targets:
                self.calls.append({"target": call_targets[node.func.id],
                                   "local_target": node.func.id,
                                   "target_span": span(node.func),
                                   "span": span(node)})
            self.generic_visit(node)

        def visit_FunctionDef(self, node):
            return

        def visit_AsyncFunctionDef(self, node):
            return

        def visit_ClassDef(self, node):
            return

    result = []
    for function in functions:
        args = function.args
        parameters = []
        all_args = list(args.posonlyargs) + list(args.args)
        if args.vararg is not None:
            all_args.append(args.vararg)
        all_args += list(args.kwonlyargs)
        if args.kwarg is not None:
            all_args.append(args.kwarg)
        all_args.sort(key=lambda argument: (argument.lineno, argument.col_offset))
        for argument in all_args:
            item = {"name": argument.arg,
                    "name_span": name_span(argument.lineno, argument.col_offset, argument.arg)}
            if argument.annotation is not None:
                item["annotation"] = source_segment(argument.annotation)
                item["annotation_span"] = span(argument.annotation)
            item["span"] = span(argument)
            parameters.append(item)

        visitor = DirectCalls()
        for statement in function.body:
            visitor.visit(statement)
        visitor.calls.sort(key=lambda call: call["span"][0])
        item = {"name": function.name,
                "name_span": name_span(function.lineno, function.col_offset, function.name),
                "span": span(function),
                "kind": "async" if isinstance(function, ast.AsyncFunctionDef) else "sync",
                "parameters": parameters,
                "local_names": sorted(local_names | set(import_targets.values())),
                "calls": visitor.calls}
        if function.returns is not None:
            item["return_type"] = source_segment(function.returns)
            item["return_type_span"] = span(function.returns)
        result.append(item)

    return {"functions": result, "imports": imports, "error": syntax_error}


def write_response(response):
    sys.stdout.write(json.dumps(response, ensure_ascii=False, separators=(",", ":")) + "\n")
    sys.stdout.flush()


write_response({"ready": True})
for request_line in sys.stdin.buffer:
    try:
        if len(request_line) > MAX_REQUEST_BYTES:
            raise ValueError("request exceeds the Python frontend limit")
        request = json.loads(request_line)
        source = request.get("source")
        if not isinstance(source, str):
            raise ValueError("request must contain a string source field")
        write_response(parse_source(source))
    except Exception as error:
        write_response({"frontend_error": str(error)})
