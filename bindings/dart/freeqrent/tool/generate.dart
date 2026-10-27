// Writes lib/src/style.g.dart, the typed Style document, from the schema
// (schema/freeqrent.schema.json, SPEC §10.3):
//
//   dart run tool/generate.dart           # write it
//   dart run tool/generate.dart --check   # exit 1 if the checked-in file is stale
//
// Every object becomes a class with optional named fields, every set of
// names a Dart enum, and every `kind`-tagged union a sealed class with one
// subclass per kind (plus a factory constructor on the base for each). Each
// type serializes to exactly the JSON the schema describes with toJson().
//
// The schema's shapes are mapped by the rules below. A shape the rules
// don't know (a new kind of union, a name that collides) stops the
// generator with a message, so the typed surface never silently drifts
// from the schema; the fix is usually one entry in the hint tables.
import 'dart:convert';
import 'dart:io';

/// Names for enums, by `Def.property` (or `Def.property[]` for an array's
/// items), where the default (Qr + the property in PascalCase) would collide
/// or read badly.
const enumNames = {
  'Frame.ring': 'QrFrameRing',
  'Caption.font': 'QrCaptionFont',
  'Logo.size': 'QrLogoSize',
  'Intro.preset': 'QrIntroName',
  'Intro.logo': 'QrLogoIntro',
  'Intro.frame': 'QrLogoIntro',
  'Intro.logo_timing': 'QrLogoTiming',
  'Intro.frame_timing': 'QrLogoTiming',
  'Style.idle[]': 'QrIdle',
  'Palette.keys': 'QrToken',
};

/// Subclass names for unions that aren't tagged by `kind`, in the order of
/// the alternatives that remain after redundant ones are dropped.
const mixedNames = {
  'QrIntro': ['Preset', 'PaletteShift'],
  'QrHalftoneClarity': ['Amount', 'Map'],
};

/// Identifiers that can't be used as they are.
const identifiers = {'class': 'cssClass', 'with': 'together'};

/// Definitions with their own names.
const defNames = {'Style': 'QrStyle', 'Image': 'QrImage'};

const reserved = {
  'abstract',
  'as',
  'assert',
  'async',
  'await',
  'break',
  'case',
  'catch',
  'class',
  'const',
  'continue',
  'default',
  'deferred',
  'do',
  'dynamic',
  'else',
  'enum',
  'export',
  'extends',
  'extension',
  'external',
  'factory',
  'false',
  'final',
  'finally',
  'for',
  'get',
  'if',
  'implements',
  'import',
  'in',
  'interface',
  'is',
  'late',
  'library',
  'mixin',
  'new',
  'null',
  'of',
  'on',
  'operator',
  'part',
  'required',
  'rethrow',
  'return',
  'sealed',
  'set',
  'static',
  'super',
  'switch',
  'this',
  'throw',
  'true',
  'try',
  'typedef',
  'var',
  'void',
  'while',
  'with',
  'yield',
  'value',
  'kind',
  'json',
};

Never fail(String why) {
  stderr.writeln('generate.dart: $why');
  exit(2);
}

String pascal(String s) => s
    .split(RegExp(r'[^A-Za-z0-9]+'))
    .where((p) => p.isNotEmpty)
    .map((p) => p[0].toUpperCase() + p.substring(1))
    .join();

String camel(String s) {
  final mapped = identifiers[s];
  if (mapped != null) return mapped;
  final p = pascal(s);
  final out = p[0].toLowerCase() + p.substring(1);
  if (reserved.contains(out)) fail('"$s" needs an entry in identifiers');
  return out;
}

/// An enum value's Dart name: `lock-on` → `lockOn`, `L` → `l`, `fg.2` → `fg2`.
String valueName(String s) {
  final mapped = identifiers[s];
  if (mapped != null) return mapped;
  final p = pascal(s);
  if (p.isEmpty) fail('enum value "$s" has no identifier');
  final out = p.toUpperCase() == p && p.length <= 2
      ? p.toLowerCase()
      : p[0].toLowerCase() + p.substring(1);
  if (reserved.contains(out) || RegExp(r'^[0-9]').hasMatch(out)) {
    fail('enum value "$s" needs an entry in identifiers');
  }
  return out;
}

String doc(Object? description, [String indent = '']) {
  if (description is! String) return '';
  return description
      .split('\n')
      .map((line) => '$indent/// $line'.trimRight())
      .join('\n')
      .replaceAll(RegExp(r'$'), '\n');
}

/// A Dart type and how to turn a value of it into JSON.
class DartType {
  DartType(this.name, this.toJson);
  final String name;
  final String Function(String expr) toJson;
}

class Field {
  Field(this.json, this.dart, this.type, this.required, this.description);
  final String json;
  final String dart;
  final DartType type;
  final bool required;
  final Object? description;
}

class Generator {
  Generator(this.schema) : defs = schema[r'$defs'] as Map<String, Object?>;

  final Map<String, Object?> schema;
  final Map<String, Object?> defs;
  final out = StringBuffer();

  /// Enum names to their values, to share identical enums and catch
  /// collisions.
  final enums = <String, List<String>>{};
  final classes = <String>{};
  final resolvedDefs = <String, DartType>{};

  Map<String, Object?> obj(Object? v) => v as Map<String, Object?>;

  DartType defType(String name) {
    final done = resolvedDefs[name];
    if (done != null) return done;
    final node = obj(defs[name]);
    final dartName = defNames[name] ?? 'Qr$name';
    final t = typeOf(node, owner: name, prop: null, name: dartName);
    return resolvedDefs[name] = t;
  }

  /// The Dart type of a schema node. [owner] and [prop] place it (for enum
  /// names); [name] is the class or union name it gets if it needs one.
  DartType typeOf(
    Map<String, Object?> node, {
    required String owner,
    required String? prop,
    required String name,
  }) {
    final ref = node[r'$ref'];
    if (ref is String) {
      final def = ref.substring(ref.lastIndexOf('/') + 1);
      if (def == 'Base64') {
        return DartType('Uint8List', (e) => 'base64Encode($e)');
      }
      if (def == 'Palette') return paletteType();
      return defType(def);
    }
    if (node['oneOf'] case final List<Object?> alts
        when node['type'] != 'object') {
      return union(alts.map(obj).toList(), owner: owner, name: name);
    }
    if (node['enum'] case final List<Object?> values) {
      final key = prop == null ? owner : '$owner.$prop';
      final enumName = enumNames[key] ?? 'Qr${pascal(prop ?? owner)}';
      return enumType(enumName, values.cast<String>());
    }
    switch (node['type']) {
      case 'string':
        return DartType('String', (e) => e);
      case 'number':
        return DartType('double', (e) => e);
      case 'integer':
        return DartType('int', (e) => e);
      case 'boolean':
        return DartType('bool', (e) => e);
      case 'array':
        final items = typeOf(
          obj(node['items']),
          owner: owner,
          prop: prop == null ? null : '$prop[]',
          name: '${name}Item',
        );
        final inner = items.toJson('x');
        return DartType(
          'List<${items.name}>',
          inner == 'x' ? (e) => e : (e) => '[for (final x in $e) $inner]',
        );
      case 'object':
        if (node['properties'] == null) {
          fail('$owner.$prop: an object without properties');
        }
        return classType(node, owner: owner, name: name);
    }
    fail('$owner.$prop: unknown schema node ${jsonEncode(node)}');
  }

  DartType paletteType() {
    final names = (obj(obj(defs['Palette'])['propertyNames'])['enum'] as List)
        .cast<String>();
    final token = enumType(enumNames['Palette.keys']!, names);
    return DartType(
      'Map<${token.name}, String>',
      (e) =>
          '{for (final MapEntry(:key, :value) in $e.entries) key.json: value}',
    );
  }

  final enumOut = StringBuffer();

  DartType enumType(String name, List<String> values) {
    final known = enums[name];
    if (known != null) {
      if (known.join('|') != values.join('|')) {
        fail('two different enums would be called $name; add a hint');
      }
    } else {
      if (classes.contains(name)) fail('$name is both a class and an enum');
      enums[name] = values;
      enumOut.writeln('enum $name {');
      for (final v in values) {
        enumOut.writeln("  ${valueName(v)}('$v'),");
      }
      enumOut.writeln('  ;\n');
      enumOut.writeln('  const $name(this.json);\n');
      enumOut.writeln('  /// The name in the Style document.');
      enumOut.writeln('  final String json;');
      enumOut.writeln('}\n');
    }
    return DartType(name, (e) => '$e.json');
  }

  final classOut = StringBuffer();

  void claim(String name) {
    if (!classes.add(name) || enums.containsKey(name)) {
      fail('two types would be called $name; add a hint');
    }
  }

  List<Field> fieldsOf(
    Map<String, Object?> node, {
    required String owner,
    required String name,
  }) {
    final props = obj(node['properties']);
    final req = ((node['required'] as List?) ?? const []).cast<String>();
    final fields = <Field>[];
    for (final MapEntry(key: json, value: v) in props.entries) {
      final p = obj(v);
      if (json == 'kind' && p.containsKey('const')) continue;
      final dart = camel(json);
      final type = typeOf(
        p,
        owner: owner,
        prop: json,
        name: '$name${pascal(json)}',
      );
      fields.add(Field(json, dart, type, req.contains(json), p['description']));
    }
    return fields;
  }

  void writeFields(StringBuffer b, List<Field> fields) {
    for (final f in fields) {
      b.write(doc(f.description, '  '));
      b.writeln('  final ${f.type.name}${f.required ? '' : '?'} ${f.dart};\n');
    }
  }

  String params(List<Field> fields, {String prefix = 'this.'}) {
    if (fields.isEmpty) return '';
    final named = fields
        .map((f) => '${f.required ? 'required ' : ''}$prefix${f.dart}')
        .join(', ');
    return '{$named}';
  }

  String typedParams(List<Field> fields) {
    if (fields.isEmpty) return '';
    final named = fields
        .map(
          (f) =>
              '${f.required ? 'required ' : ''}${f.type.name}${f.required ? '' : '?'} ${f.dart}',
        )
        .join(', ');
    return '{$named}';
  }

  /// With [bare], a kind whose fields are all left out writes its bare
  /// name, as the schema allows.
  void writeToJson(
    StringBuffer b,
    List<Field> fields, {
    String? kind,
    bool bare = false,
  }) {
    b.writeln('  @override');
    if (bare && fields.every((f) => !f.required)) {
      if (fields.isEmpty) {
        b.writeln("  Object? toJson() => '$kind';");
        return;
      }
      final none = fields.map((f) => '${f.dart} == null').join(' && ');
      b.writeln("  Object? toJson() => $none ? '$kind' : {");
    } else {
      b.writeln('  Map<String, Object?> toJson() => {');
    }
    if (kind != null) b.writeln("    'kind': '$kind',");
    for (final f in fields) {
      final value = f.type.toJson(f.required ? f.dart : '${f.dart}!');
      if (f.required) {
        b.writeln("    '${f.json}': $value,");
      } else {
        b.writeln("    if (${f.dart} != null) '${f.json}': $value,");
      }
    }
    b.writeln('  };');
  }

  DartType classType(
    Map<String, Object?> node, {
    required String owner,
    required String name,
  }) {
    claim(name);
    final fields = fieldsOf(node, owner: _ownerOf(name), name: name);
    final b = StringBuffer();
    b.write(doc(node['description']));
    b.writeln('class $name extends QrDocument {');
    b.writeln(
      '  /// ${name == 'QrStyle' ? 'A Style document; every part is optional.' : 'Its fields; those left out take the core\'s defaults.'}',
    );
    b.writeln('  const $name(${params(fields)});\n');
    writeFields(b, fields);
    writeToJson(b, fields);
    b.writeln('}\n');
    classOut.write(b);
    return DartType(name, (e) => '$e.toJson()');
  }

  /// The schema definition a generated name belongs to, for enum names.
  String _ownerOf(String name) {
    for (final MapEntry(:key, :value) in defNames.entries) {
      if (value == name) return key;
    }
    return name.startsWith('Qr') ? name.substring(2) : name;
  }

  DartType union(
    List<Map<String, Object?>> alts, {
    required String owner,
    required String name,
  }) {
    String? kindOf(Map<String, Object?> a) {
      if (a['type'] != 'object') return null;
      final kind = obj(a['properties'] ?? const {})['kind'];
      return kind is Map ? kind['const'] as String? : null;
    }

    final kinds = alts.map(kindOf).whereType<String>().toList();
    final enumAlts = alts.where((a) => a['enum'] != null).toList();
    final rest = alts
        .where((a) => a['enum'] == null && kindOf(a) == null)
        .toList();

    if (kinds.isNotEmpty && rest.isEmpty) {
      for (final e in enumAlts) {
        for (final v in (e['enum'] as List).cast<String>()) {
          if (!kinds.contains(v))
            fail('$name: bare name $v has no kind object');
        }
      }
      return kindUnion(
        alts.where((a) => kindOf(a) != null).toList(),
        owner: owner,
        name: name,
        bare: {for (final e in enumAlts) ...(e['enum'] as List).cast<String>()},
      );
    }
    if (kinds.isNotEmpty) fail('$name: kinds mixed with other shapes');

    // A bare name the object form also takes, or text an object carries as
    // a required string, is shorthand: the typed surface keeps the object.
    final objects = rest.where((a) => a['type'] == 'object').toList();
    final kept = rest.where((a) {
      if (a['type'] == 'string' && a['enum'] == null) {
        return !objects.any((o) {
          final req = ((o['required'] as List?) ?? const []).cast<String>();
          final props = obj(o['properties'] ?? const {});
          return req.any((r) => obj(props[r])['type'] == 'string');
        });
      }
      return true;
    }).toList();
    for (final e in enumAlts) {
      final values = (e['enum'] as List).join('|');
      final covered = objects.any(
        (o) => obj(
          o['properties'] ?? const {},
        ).values.any((p) => (obj(p)['enum'] as List?)?.join('|') == values),
      );
      if (!covered) fail('$name: bare names $values have no object form');
    }
    if (kept.length == 1) {
      return typeOf(kept.single, owner: owner, prop: null, name: name);
    }
    final subs = mixedNames[name];
    if (subs == null || subs.length != kept.length) {
      fail('$name: a union of ${kept.length} shapes needs mixedNames');
    }
    claim(name);
    final b = StringBuffer();
    final subDefs = StringBuffer();
    b.writeln('sealed class $name extends QrDocument {');
    b.writeln('  const $name();\n');
    for (var i = 0; i < kept.length; i++) {
      final a = kept[i];
      final sub = '$name${subs[i]}';
      final factory = camel(subs[i]);
      if (a['type'] == 'object' && a['properties'] != null) {
        claim(sub);
        final fields = fieldsOf(a, owner: owner, name: sub);
        b.write(doc(a['description'], '  '));
        b.writeln(
          '  const factory $name.$factory(${typedParams(fields)}) = $sub;\n',
        );
        subDefs.write(doc(a['description']));
        subDefs.writeln('final class $sub extends $name {');
        subDefs.writeln(
          '  /// Its fields; those left out take the core\'s defaults.',
        );
        subDefs.writeln('  const $sub(${params(fields)});\n');
        writeFields(subDefs, fields);
        writeToJson(subDefs, fields);
        subDefs.writeln('}\n');
      } else {
        claim(sub);
        final t = typeOf(a, owner: owner, prop: null, name: '${sub}Value');
        b.writeln('  const factory $name.$factory(${t.name} value) = $sub;\n');
        subDefs.writeln('final class $sub extends $name {');
        subDefs.writeln('  /// The value as it is.');
        subDefs.writeln('  const $sub(this.value);\n');
        subDefs.writeln('  /// The value.');
        subDefs.writeln('  final ${t.name} value;\n');
        subDefs.writeln('  @override');
        subDefs.writeln('  Object? toJson() => ${t.toJson('value')};');
        subDefs.writeln('}\n');
      }
    }
    b.writeln('}\n');
    classOut.write(b);
    classOut.write(subDefs);
    return DartType(name, (e) => '$e.toJson()');
  }

  DartType kindUnion(
    List<Map<String, Object?>> alts, {
    required String owner,
    required String name,
    required Set<String> bare,
  }) {
    claim(name);
    final b = StringBuffer();
    final subDefs = StringBuffer();
    b.writeln('sealed class $name extends QrDocument {');
    b.writeln('  const $name();\n');
    for (final a in alts) {
      final kind = obj(obj(a['properties'])['kind'])['const'] as String;
      final sub = '$name${pascal(identifiers[kind] ?? kind)}';
      claim(sub);
      final fields = fieldsOf(a, owner: owner, name: sub);
      b.writeln('  /// `$kind`.');
      b.writeln(
        '  const factory $name.${camel(kind)}(${typedParams(fields)}) = $sub;\n',
      );
      subDefs.writeln('/// `{"kind": "$kind"}`.');
      subDefs.writeln('final class $sub extends $name {');
      subDefs.writeln(
        '  /// Its fields; those left out take the core\'s defaults.',
      );
      subDefs.writeln('  const $sub(${params(fields)});\n');
      writeFields(subDefs, fields);
      writeToJson(subDefs, fields, kind: kind, bare: bare.contains(kind));
      subDefs.writeln('}\n');
    }
    b.writeln('}\n');
    classOut.write(b);
    classOut.write(subDefs);
    return DartType(name, (e) => '$e.toJson()');
  }

  String run() {
    defType('Style');
    defType('PaletteOpts');
    final version = schema['x-freeqrent-version'];
    return '''
// GENERATED by tool/generate.dart from schema/freeqrent.schema.json
// (freeQRent $version). Don't edit it by hand: change the schema (or the
// generator) and run `dart run tool/generate.dart`.
//
// ignore_for_file: prefer_collection_literals, unnecessary_brace_in_string_interps

import 'dart:convert';
import 'dart:typed_data';

/// A part of a Style document (SPEC §10.1) that serializes to its JSON.
abstract class QrDocument {
  /// A part of a document.
  const QrDocument();

  /// The part as the JSON value the Style document takes.
  Object? toJson();

  /// The part as JSON text.
  String toJsonString() => jsonEncode(toJson());
}

$enumOut$classOut'''
        .replaceAll(RegExp(r'\n{3,}'), '\n\n')
        .replaceAll(RegExp(r'\n+$'), '\n');
  }
}

/// The SDK's formatter over [source], so the checked-in file is exactly
/// what `dart format` leaves.
String format(String source, String root) {
  final tmp = Directory.systemTemp.createTempSync('freeqrent-gen');
  try {
    final file = File('${tmp.path}/style.g.dart')..writeAsStringSync(source);
    // The package's language version decides the style, so copy its
    // pubspec next to the file.
    File('$root/pubspec.yaml').copySync('${tmp.path}/pubspec.yaml');
    final p = Process.runSync(Platform.resolvedExecutable, [
      'format',
      '--language-version=3.9',
      file.path,
    ]);
    if (p.exitCode != 0) fail('dart format failed:\n${p.stderr}');
    return file.readAsStringSync();
  } finally {
    tmp.deleteSync(recursive: true);
  }
}

void main(List<String> args) {
  final root = File.fromUri(Platform.script).parent.parent;
  final schemaFile = File('${root.path}/../../../schema/freeqrent.schema.json');
  final target = File('${root.path}/lib/src/style.g.dart');
  final schema =
      jsonDecode(schemaFile.readAsStringSync()) as Map<String, Object?>;
  final text = format(Generator(schema).run(), root.path);
  if (args.contains('--check')) {
    if (!target.existsSync() || target.readAsStringSync() != text) {
      stderr.writeln(
        '${target.path} is stale: run `dart run tool/generate.dart`',
      );
      exit(1);
    }
    return;
  }
  target.writeAsStringSync(text);
  stdout.writeln('wrote ${target.path}');
}
