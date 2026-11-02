// `goldens`: the inputs of Roslyn's own parser and lexer tests (src/Compilers/CSharp/Test/Syntax/Parsing and
// LexicalAndXml at the pin), found by parsing the test sources with Roslyn itself, each dumped with the options and
// the entry point its test parses it with (README.md, "Goldens").
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace CSharpOracle;

/// <summary>Parse options as a test writes them: a version as asked (<c>LanguageVersion.Default</c> when the test
/// names none), the documentation mode, and what puts an input out of scope (script, preprocessor symbols,
/// features).</summary>
readonly record struct TestOptions(LanguageVersion Version, DocumentationMode Doc, bool Script, bool Symbols, bool Features)
{
    public static readonly TestOptions Regular = new(LanguageVersion.Default, DocumentationMode.Parse, false, false, false);

    public TestOptions WithVersion(LanguageVersion v) => this with { Version = v };

    /// <summary><c>TestOptions.*</c> of src/Compilers/Test/Utilities/CSharp/TestOptions.cs at the pin.</summary>
    public static TestOptions? Named(string name)
    {
        var r = Regular;
        return name switch
        {
            "Regular" or "RegularDefault" => r,
            "Script" => r with { Script = true },
            "Regular1" => r.WithVersion(LanguageVersion.CSharp1),
            "Regular2" => r.WithVersion(LanguageVersion.CSharp2),
            "Regular3" => r.WithVersion(LanguageVersion.CSharp3),
            "Regular4" => r.WithVersion(LanguageVersion.CSharp4),
            "Regular5" => r.WithVersion(LanguageVersion.CSharp5),
            "Regular6" => r.WithVersion(LanguageVersion.CSharp6),
            "Regular7" => r.WithVersion(LanguageVersion.CSharp7),
            "Regular7_1" => r.WithVersion(LanguageVersion.CSharp7_1),
            "Regular7_2" or "WithoutImprovedOverloadCandidates" => r.WithVersion(LanguageVersion.CSharp7_2),
            "Regular7_3" or "RegularWithoutRecursivePatterns" => r.WithVersion(LanguageVersion.CSharp7_3),
            "Regular8" or "RegularWithRecursivePatterns" or "RegularWithoutPatternCombinators" or "WithoutCovariantReturns"
                => r.WithVersion(LanguageVersion.CSharp8),
            "Regular9" or "WithCovariantReturns" => r.WithVersion(LanguageVersion.CSharp9),
            "Regular10" or "RegularWithFileScopedNamespaces" => r.WithVersion(LanguageVersion.CSharp10),
            "Regular11" => r.WithVersion(LanguageVersion.CSharp11),
            "Regular12" => r.WithVersion(LanguageVersion.CSharp12),
            "Regular13" => r.WithVersion(LanguageVersion.CSharp13),
            "Regular14" => r.WithVersion(LanguageVersion.CSharp14),
            "RegularNext" or "RegularPreview" or "RegularWithExtendedPartialMethods" or "RegularWithPatternCombinators"
                or "RegularWithExtendedPropertyPatterns" or "RegularWithListPatterns" => r.WithVersion(LanguageVersion.Preview),
            "RegularWithDocumentationComments" => r with { Doc = DocumentationMode.Diagnose },
            "RegularPreviewWithDocumentationComments" =>
                r with { Version = LanguageVersion.Preview, Doc = DocumentationMode.Diagnose },
            "ExperimentalParseOptions" => r with { Version = LanguageVersion.Preview, Doc = DocumentationMode.None },
            "RegularWithLegacyStrongName" => r with { Features = true },
            _ => null,
        };
    }
}

static class Goldens
{
    const string TestRoot = "src/Compilers/CSharp/Test/Syntax";

    /// <summary>A golden whose dump is longer is left out (ExpressionParsingTests.ParseBigExpression's is 20 MB):
    /// the robustness tests and the corpora cover size.</summary>
    const int MaxDump = 1 << 20;
    static readonly string[] s_dirs = { "Parsing", "LexicalAndXml" };

    /// <summary>How a test class parses through the virtual <c>ParseTree</c>/<c>ParseNode</c> of ParsingTests:
    /// <c>Tree</c> maps the options a test passes (null: none) to what <c>ParseTree</c> parses with (null: the class
    /// cannot); <c>Node</c> is the entry of <c>ParseNode</c> (null: <c>ParseTree</c>'s compilation unit) with its
    /// options and the text it really parses (<c>Wrap</c>, a format of the test's text).</summary>
    sealed record ClassConfig(Func<TestOptions?, TestOptions?> Tree, Entry? Node, Func<TestOptions?, TestOptions>? NodeOptions,
        string? Wrap = null);

    static readonly ClassConfig s_default = new(o => o ?? TestOptions.Regular, null, null);

    static ClassConfig ExpressionNode(Func<TestOptions?, TestOptions> o) => new(x => o(x), Entry.Expression, o);

    /// <summary>The overrides of ParseTree and ParseNode in the test classes at the pin.</summary>
    static readonly Dictionary<string, ClassConfig> s_classes = new()
    {
        ["UsingDirectiveParsingTests"] = new(o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.Preview), null, null),
        ["ScriptParsingTests"] = new(o => o ?? TestOptions.Regular with { Script = true }, null, null),
        ["AsyncParsingTests"] = ExpressionNode(o => (o ?? TestOptions.Regular).WithVersion(LanguageVersion.CSharp5)),
        ["AsyncStreamsParsingTests"] = ExpressionNode(o => (o ?? TestOptions.Regular).WithVersion(LanguageVersion.CSharp8)),
        ["AwaitParsingTests"] = ExpressionNode(_ => TestOptions.Regular),
        ["NullConditionalAssignmentParsingTests"] = ExpressionNode(o => o ?? TestOptions.Regular),
        ["LambdaAttributeParsingTests"] = ExpressionNode(o => o ?? TestOptions.Regular),
        ["LineSpanDirectiveParsingTests"] = ExpressionNode(o => o ?? TestOptions.Regular),
        ["LambdaParameterParsingTests"] = ExpressionNode(o => o ?? TestOptions.Regular),
        ["LambdaReturnTypeParsingTests"] = ExpressionNode(o => o ?? TestOptions.Regular),
        ["SuppressNullableWarningExpressionParsingTests"] =
            new(o => o ?? TestOptions.Regular, Entry.Expression, o => o ?? TestOptions.Regular),
        // ParseNode wraps the text in a doc comment and lexes it as leading trivia (SyntaxFactory.ParseLeadingTrivia):
        // the tokens entry of that text is the same lexer call, its trivia those of the end-of-file token.
        ["CrefParsingTests"] = new(_ => null, Entry.Tokens, o => o ?? TestOptions.Regular, "/// <see cref=\"{0}\"/>"),
        ["NameAttributeValueParsingTests"] = new(_ => null, Entry.Tokens, _ => TestOptions.Regular, "/// <param name=\"{0}\"/>"),
        ["VerbatimCrefParsingTests"] = new(_ => null, Entry.Tokens, _ => TestOptions.Regular, "/// <see cref=\"{0}\"/>"),
    };

    /// <summary>The private <c>new</c> helpers of test classes that pass fixed options when a call gives none (a call
    /// with options binds the base helper), and the two classes whose <c>UsingNode</c> parses a tree with
    /// <c>options ?? TestOptions.RegularPreview</c>.</summary>
    static readonly Dictionary<(string Class, string Helper), Func<TestOptions?, TestOptions?>> s_wrappers = new()
    {
        [("PatternParsingTests", "UsingStatement")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.CSharp8),
        [("PatternParsingTests2", "UsingExpression")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.Preview),
        [("PatternParsingTests_ListPatterns", "UsingExpression")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.Preview),
        [("RecordParsingTests", "UsingTree")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.CSharp9),
        [("RecordParsingTests", "UsingExpression")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.CSharp9),
        [("RecordParsingTests", "UsingStatement")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.CSharp9),
        [("FileModifierParsingTests", "UsingNode")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.Preview),
        [("ClosedModifierParsingTests", "UsingNode")] = o => o ?? TestOptions.Regular.WithVersion(LanguageVersion.Preview),
    };

    enum Via { Tree, Node, Direct }

    /// <summary>A recognized call: how it parses, and its parameters in order. Named arguments bind by name; a
    /// parameter ending in <c>?</c> may be absent (an <c>offset?</c> is one only when it is a number, an
    /// <c>options?</c> only when it is not the first expected diagnostic); <c>defines...</c> takes the rest.</summary>
    sealed record Call(Via Via, Entry Entry, string[] Params, string? Class = null, TestOptions? Default = null,
        Func<TestOptions?, TestOptions?>? Wrapper = null);

    static readonly string[] s_textOptions = { "text", "options?" };
    static readonly string[] s_factory = { "text", "offset", "options", "consumeFullText" };

    static Call? Recognize(string name, string? qualifier, string cls)
    {
        var self = qualifier is null or "this" or "base";
        if (qualifier is "SyntaxFactory")
        {
            return name switch
            {
                "ParseSyntaxTree" => new Call(Via.Direct, Entry.CompilationUnit, new[] { "text", "options" }),
                "ParseCompilationUnit" => new Call(Via.Direct, Entry.CompilationUnit, new[] { "text", "offset", "options" }),
                "ParseExpression" => new Call(Via.Direct, Entry.Expression, s_factory),
                "ParseStatement" => new Call(Via.Direct, Entry.Statement, s_factory),
                "ParseMemberDeclaration" => new Call(Via.Direct, Entry.Member, s_factory),
                "ParseTypeName" => new Call(Via.Direct, Entry.TypeName, s_factory),
                "ParseName" => new Call(Via.Direct, Entry.Name, new[] { "text", "offset", "consumeFullText" }),
                "ParseParameterList" => new Call(Via.Direct, Entry.ParameterList, s_factory),
                "ParseBracketedParameterList" => new Call(Via.Direct, Entry.BracketedParameterList, s_factory),
                "ParseArgumentList" => new Call(Via.Direct, Entry.ArgumentList, s_factory),
                "ParseBracketedArgumentList" => new Call(Via.Direct, Entry.BracketedArgumentList, s_factory),
                "ParseAttributeArgumentList" => new Call(Via.Direct, Entry.AttributeArgumentList, s_factory),
                "ParseTokens" => new Call(Via.Direct, Entry.Tokens, new[] { "text", "offset", "initialTokenPosition", "options" }),
                "ParseLeadingTrivia" => new Call(Via.Direct, Entry.Tokens, new[] { "text", "offset" }),
                _ => null,
            };
        }
        if (qualifier is "CSharpSyntaxTree" && name == "ParseText")
        {
            return new Call(Via.Direct, Entry.CompilationUnit, new[] { "text", "options" });
        }
        if (name == "ParseAndRoundTripping" && qualifier == "RoundTrippingTests")
        {
            return Recognize(name, null, qualifier);
        }
        if (name is "ParseAndValidate" or "ParseAndValidateFirst" or "ParseWithRoundTripCheck" &&
            (self || qualifier is "ParsingTests" or "ParserErrorMessageTests" or "CSharpTestBase"))
        {
            return new Call(Via.Direct, Entry.CompilationUnit, s_textOptions);
        }
        if (!self)
        {
            return null;
        }
        var wrapper = s_wrappers.GetValueOrDefault((cls, name));
        return (name, cls) switch
        {
            ("Parse" or "ParseTree", "PreprocessorTests") =>
                new Call(Via.Direct, Entry.CompilationUnit, new[] { "text", "defines..." }, cls),
            ("Parse", "XmlDocCommentTests") => new Call(Via.Direct, Entry.CompilationUnit, new[] { "text", "defines..." }, cls),
            ("Lex" or "LexToken", "LexicalTests") => new Call(Via.Direct, Entry.Tokens, new[] { "text", "options" }),
            // The private helpers of single test classes, each a SyntaxFactory call.
            ("ParseStatement", "StatementParsingTests") => new Call(Via.Direct, Entry.Statement, new[] { "text", "offset", "options" }),
            ("ParseDeclaration", "MemberDeclarationParsingTests") =>
                new Call(Via.Direct, Entry.Member, new[] { "text", "offset", "options" }),
            ("ParseExpression", "ExpressionParsingTests") => new Call(Via.Direct, Entry.Expression, new[] { "text", "options" }),
            ("ParseName", "NameParsingTests") => new Call(Via.Direct, Entry.Name, new[] { "text" }),
            ("ParseTypeName", "NameParsingTests") => new Call(Via.Direct, Entry.TypeName, new[] { "text" }),
            ("UsingLineDirective", "LineSpanDirectiveParsingTests") => new Call(Via.Tree, Entry.CompilationUnit, s_textOptions),
            ("ParseAndRoundTripping", "RoundTrippingTests") => new Call(Via.Direct, Entry.CompilationUnit, s_textOptions,
                Default: TestOptions.Regular with { Version = LanguageVersion.Preview, Doc = DocumentationMode.Diagnose }),
            ("UsingTree" or "ParseTree", _) => new Call(Via.Tree, Entry.CompilationUnit, s_textOptions, Wrapper: wrapper),
            ("UsingNode", "FileModifierParsingTests" or "ClosedModifierParsingTests") =>
                new Call(Via.Direct, Entry.CompilationUnit, s_textOptions, Wrapper: s_wrappers[(cls, name)]),
            ("UsingNode", _) => new Call(Via.Node, Entry.CompilationUnit, s_textOptions),
            ("ParseFile", _) => new Call(Via.Direct, Entry.CompilationUnit, new[] { "text", "parseOptions" }),
            ("UsingExpression", _) => new Call(Via.Direct, Entry.Expression, s_textOptions, Wrapper: wrapper),
            ("UsingStatement", _) => new Call(Via.Direct, Entry.Statement, s_textOptions, Wrapper: wrapper),
            ("UsingDeclaration", _) =>
                new Call(Via.Direct, Entry.Member, new[] { "text", "offset?", "options?", "consumeFullText" }),
            _ => null,
        };
    }

    sealed record Golden(string Class, string Member, string Text, Entry Entry, LanguageVersion Version, DocumentationMode Doc,
        string Source)
    {
        public string Name = "";
    }

    sealed class Context(CSharpCompilation compilation)
    {
        readonly Dictionary<SyntaxTree, SemanticModel> _models = new();
        public readonly SortedDictionary<string, int> Skipped = new(StringComparer.Ordinal);
        public readonly Dictionary<string, int> Unrecognized = new();
        public readonly StringBuilder Log = new();
        public InvocationExpressionSyntax? Current;

        /// <summary>The theory row being extracted: the test method and its arguments by parameter name.</summary>
        public MethodDeclarationSyntax? Method;
        public Dictionary<string, ExpressionSyntax>? Row;

        /// <summary>The element of each enclosing <c>foreach</c> over a literal array, by its variable.</summary>
        public Dictionary<ForEachStatementSyntax, ExpressionSyntax>? Loop;

        public bool Contains(SyntaxTree tree) => compilation.ContainsSyntaxTree(tree);

        public SemanticModel Model(SyntaxNode n)
        {
            if (!_models.TryGetValue(n.SyntaxTree, out var m))
            {
                m = compilation.GetSemanticModel(n.SyntaxTree);
                _models[n.SyntaxTree] = m;
            }
            return m;
        }

        public void Skip(string reason)
        {
            Skipped[reason] = Skipped.GetValueOrDefault(reason) + 1;
            if (Current != null)
            {
                var line = Current.GetLocation().GetLineSpan().StartLinePosition.Line + 1;
                var call = Current.ToString().Replace('\n', ' ').Replace('\r', ' ').Replace('\t', ' ');
                Log.Append(reason).Append('\t').Append(Current.SyntaxTree.FilePath).Append(':').Append(line).Append('\t')
                    .Append(call.Length > 160 ? call[..160] : call).Append('\n');
            }
        }
    }

    public static int Run(Args a)
    {
        var src = a.Need("src");
        var outDir = a.Need("out");
        var parseOptions = new CSharpParseOptions(LanguageVersion.Preview);
        var trees = new List<SyntaxTree>();
        foreach (var dir in s_dirs)
        {
            var full = Path.Combine(src, TestRoot, dir);
            foreach (var file in Directory.GetFiles(full, "*.cs").OrderBy(f => f, StringComparer.Ordinal))
            {
                var rel = dir + "/" + Path.GetFileName(file);
                trees.Add(CSharpSyntaxTree.ParseText(File.ReadAllText(file, Encoding.UTF8), parseOptions, rel));
            }
        }
        var tpa = ((string?)AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES") ?? "").Split(Path.PathSeparator)
            .Where(p => p.Length > 0).OrderBy(p => p, StringComparer.Ordinal).Select(p => MetadataReference.CreateFromFile(p));
        var compilation = CSharpCompilation.Create("tests", trees, tpa,
            new CSharpCompilationOptions(OutputKind.DynamicallyLinkedLibrary, nullableContextOptions: NullableContextOptions.Enable));
        var cx = new Context(compilation);
        if (a.Has("check-interpolation"))
        {
            CheckInterpolation(cx, trees);
        }
        var found = new List<Golden>();
        var calls = 0;
        foreach (var tree in trees)
        {
            foreach (var inv in tree.GetRoot().DescendantNodes().OfType<InvocationExpressionSyntax>())
            {
                var (name, qualifier) = inv.Expression switch
                {
                    IdentifierNameSyntax id => (id.Identifier.ValueText, (string?)null),
                    MemberAccessExpressionSyntax { Name: IdentifierNameSyntax n } ma => (n.Identifier.ValueText, ma.Expression.ToString()),
                    _ => ("", null),
                };
                var type = inv.Ancestors().OfType<TypeDeclarationSyntax>().FirstOrDefault();
                if (name == "" || type == null)
                {
                    continue;
                }
                var cls = type.Identifier.ValueText;
                var call = Recognize(name, qualifier, cls);
                if (call == null)
                {
                    if ((name.StartsWith("Parse", StringComparison.Ordinal) || name.StartsWith("Using", StringComparison.Ordinal) ||
                         name.StartsWith("Lex", StringComparison.Ordinal) || name.StartsWith("Create", StringComparison.Ordinal)) &&
                        inv.ArgumentList.Arguments.Count > 0 && EvalText(cx, inv.ArgumentList.Arguments[0].Expression) != null)
                    {
                        var key = (qualifier == null ? "" : qualifier + ".") + name;
                        cx.Unrecognized[key] = cx.Unrecognized.GetValueOrDefault(key) + 1;
                    }
                    continue;
                }
                calls++;
                cx.Current = inv;
                cx.Method = inv.Ancestors().OfType<MethodDeclarationSyntax>().LastOrDefault(m => m.Parent == type);
                foreach (var row in TheoryRows(cx.Method))
                {
                    foreach (var loop in LoopRows(inv))
                    {
                        cx.Row = row;
                        cx.Loop = loop;
                        if (Extract(cx, inv, call, cls, MemberName(inv, type)) is { } g)
                        {
                            found.Add(g);
                        }
                    }
                }
                cx.Row = null;
                cx.Loop = null;
            }
        }

        var goldens = new List<Golden>();
        var seen = new HashSet<(string, Entry, string, DocumentationMode)>();
        foreach (var g in found)
        {
            if (seen.Add((g.Text, g.Entry, Options.VersionName(g.Version), g.Doc)))
            {
                goldens.Add(g);
            }
            else
            {
                cx.Skipped["duplicate input"] = cx.Skipped.GetValueOrDefault("duplicate input") + 1;
            }
        }
        cx.Current = null;
        Name(goldens);
        if (a.Get("skipped") is { } skippedLog)
        {
            File.WriteAllText(skippedLog, cx.Log.ToString(), new UTF8Encoding(false));
        }

        // Write: clear what an earlier run wrote, then every input and its dump.
        Directory.CreateDirectory(outDir);
        foreach (var f in Directory.GetFiles(outDir, "*", SearchOption.AllDirectories))
        {
            if (f.EndsWith(".cs", StringComparison.Ordinal) || f.EndsWith(".tree.txt", StringComparison.Ordinal) ||
                Path.GetFileName(f) == "manifest.tsv")
            {
                File.Delete(f);
            }
        }
        foreach (var d in Directory.GetDirectories(outDir).Where(d => !Directory.EnumerateFileSystemEntries(d).Any()))
        {
            Directory.Delete(d);
        }
        var utf8 = new UTF8Encoding(false);
        var results = new DumpResult[goldens.Count];
        Workers.Run(goldens.Count, a.Threads, a.StackBytes, i =>
        {
            var g = goldens[i];
            try
            {
                results[i] = Dumper.Dump(g.Text, g.Entry, new Options(g.Version, g.Doc));
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                results[i] = new DumpResult(null, 0, null, 0, e.GetType().Name);
            }
        });
        var manifest = new StringBuilder();
        var counts = new SortedDictionary<string, int>(StringComparer.Ordinal);
        var written = 0;
        for (var i = 0; i < goldens.Count; i++)
        {
            var g = goldens[i];
            var r = results[i];
            if (r.Text == null)
            {
                cx.Skip("oracle: " + r.Failure);
                continue;
            }
            if (r.Text.Length > MaxDump)
            {
                cx.Skip("dump over 1 MB");
                continue;
            }
            var path = Path.Combine(outDir, g.Name);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllBytes(path + ".cs", utf8.GetBytes(g.Text));
            File.WriteAllBytes(path + ".tree.txt", utf8.GetBytes(r.Text));
            var version = Options.VersionName(g.Version);
            var doc = Options.DocName(g.Doc);
            manifest.Append(g.Name).Append('\t').Append(version).Append('\t').Append(Options.EntryName(g.Entry))
                .Append('\t').Append(doc).Append('\t').Append(g.Source).Append('\n');
            foreach (var k in new[]
                     {
                         "entry " + Options.EntryName(g.Entry), "version " + version, "doc " + doc,
                         r.Errors > 0 ? "with diagnostics" : "without diagnostics",
                     })
            {
                counts[k] = counts.GetValueOrDefault(k) + 1;
            }
            written++;
        }
        File.WriteAllText(Path.Combine(outDir, "manifest.tsv"), manifest.ToString(), utf8);

        var e = Console.Error;
        e.WriteLine($"{trees.Count} test files, {calls} recognized calls, {found.Count} constant inputs, {written} goldens");
        foreach (var (k, v) in counts)
        {
            e.WriteLine($"  {v,6}  {k}");
        }
        e.WriteLine("skipped:");
        foreach (var (k, v) in cx.Skipped)
        {
            e.WriteLine($"  {v,6}  {k}");
        }
        e.WriteLine("unrecognized calls with a constant first argument:");
        foreach (var (k, v) in cx.Unrecognized.OrderByDescending(kv => kv.Value).ThenBy(kv => kv.Key, StringComparer.Ordinal).Take(40))
        {
            e.WriteLine($"  {v,6}  {k}");
        }
        return 0;
    }

    static readonly ExpressionSyntax[] s_bools = { SyntaxFactory.ParseExpression("false"), SyntaxFactory.ParseExpression("true") };

    /// <summary>The argument rows of a theory: each <c>[InlineData(...)]</c>, or for <c>[CombinatorialData]</c> every
    /// combination of each parameter's <c>[CombinatorialValues(...)]</c> (a bool: false, true), at most 64; one empty
    /// row for anything else.</summary>
    static List<Dictionary<string, ExpressionSyntax>?> TheoryRows(MethodDeclarationSyntax? method)
    {
        var none = new List<Dictionary<string, ExpressionSyntax>?> { null };
        if (method == null || method.ParameterList.Parameters.Count == 0)
        {
            return none;
        }
        var ps = method.ParameterList.Parameters;
        var attrs = method.AttributeLists.SelectMany(l => l.Attributes).ToList();
        static string Simple(AttributeSyntax a) => a.Name.ToString().Split('.')[^1];
        var rows = new List<Dictionary<string, ExpressionSyntax>?>();
        foreach (var a in attrs.Where(a => Simple(a) is "InlineData" or "InlineDataAttribute"))
        {
            var args = a.ArgumentList?.Arguments.Where(x => x.NameEquals == null).ToList() ?? new();
            var row = new Dictionary<string, ExpressionSyntax>();
            for (var i = 0; i < Math.Min(args.Count, ps.Count); i++)
            {
                row[ps[i].Identifier.ValueText] = args[i].Expression;
            }
            rows.Add(row);
        }
        foreach (var a in attrs.Where(a => Simple(a) == "MemberData"))
        {
            if (a.ArgumentList?.Arguments.FirstOrDefault()?.Expression is InvocationExpressionSyntax
                {
                    Expression: IdentifierNameSyntax { Identifier.ValueText: "nameof" },
                    ArgumentList.Arguments: [{ Expression: IdentifierNameSyntax target }],
                } && MemberDataRows(method, target.Identifier.ValueText) is { } data)
            {
                foreach (var values in data)
                {
                    var row = new Dictionary<string, ExpressionSyntax>();
                    for (var i = 0; i < Math.Min(values.Count, ps.Count); i++)
                    {
                        row[ps[i].Identifier.ValueText] = values[i];
                    }
                    rows.Add(row);
                }
            }
        }
        if (rows.Count == 0 && attrs.Any(a => Simple(a) == "CombinatorialData"))
        {
            var combos = new List<Dictionary<string, ExpressionSyntax>> { new() };
            foreach (var p in ps)
            {
                var values = p.AttributeLists.SelectMany(l => l.Attributes).FirstOrDefault(a => Simple(a) == "CombinatorialValues")
                    ?.ArgumentList?.Arguments.Select(x => x.Expression).ToList();
                if (values == null && p.Type?.ToString() == "bool")
                {
                    values = s_bools.ToList();
                }
                if (values == null || values.Count * combos.Count > 64)
                {
                    return none;
                }
                combos = combos.SelectMany(c => values.Select(v => new Dictionary<string, ExpressionSyntax>(c) { [p.Identifier.ValueText] = v })).ToList();
            }
            rows.AddRange(combos);
        }
        return rows.Count > 0 ? rows : none;
    }

    /// <summary>The rows of a field or property initialized with <c>new TheoryData&lt;T&gt;([a, b])</c>,
    /// <c>new TheoryData&lt;T, U&gt; { { a, b }, ... }</c> or an array of argument arrays.</summary>
    static List<List<ExpressionSyntax>>? MemberDataRows(MethodDeclarationSyntax method, string name)
    {
        var type = method.Ancestors().OfType<TypeDeclarationSyntax>().FirstOrDefault();
        var init = type?.Members.Select(m => m switch
        {
            FieldDeclarationSyntax f => f.Declaration.Variables.FirstOrDefault(v => v.Identifier.ValueText == name)?.Initializer?.Value,
            PropertyDeclarationSyntax p when p.Identifier.ValueText == name => p.Initializer?.Value ?? p.ExpressionBody?.Expression,
            _ => null,
        }).FirstOrDefault(e => e != null);
        static IEnumerable<ExpressionSyntax>? Elements(ExpressionSyntax? e) => e switch
        {
            CollectionExpressionSyntax c => c.Elements.All(x => x is ExpressionElementSyntax)
                ? c.Elements.Cast<ExpressionElementSyntax>().Select(x => x.Expression)
                : null,
            ArrayCreationExpressionSyntax a => a.Initializer?.Expressions,
            ImplicitArrayCreationExpressionSyntax a => a.Initializer.Expressions,
            InitializerExpressionSyntax i => i.Expressions,
            _ => null,
        };
        switch (init)
        {
            case BaseObjectCreationExpressionSyntax oc when oc is not ObjectCreationExpressionSyntax || oc is ObjectCreationExpressionSyntax { Type: GenericNameSyntax { Identifier.ValueText: "TheoryData" } }:
            {
                var items = oc.ArgumentList?.Arguments.Count == 1 ? Elements(oc.ArgumentList.Arguments[0].Expression) : Elements(oc.Initializer);
                return items?.Select(x => x is InitializerExpressionSyntax { RawKind: (int)SyntaxKind.ComplexElementInitializerExpression } c
                    ? c.Expressions.ToList()
                    : new List<ExpressionSyntax> { x }).ToList();
            }
            case ArrayCreationExpressionSyntax or ImplicitArrayCreationExpressionSyntax or CollectionExpressionSyntax:
            {
                var rows = Elements(init)?.Select(r => Elements(r)?.ToList()).ToList();
                return rows == null || rows.Any(r => r == null) ? null : rows.Select(r => r!).ToList();
            }
            default:
                return null;
        }
    }

    /// <summary>Every combination of the elements of the enclosing <c>foreach</c> loops over literal arrays
    /// (<c>foreach (var options in new[] { TestOptions.Regular13, TestOptions.RegularPreview })</c>), at most 64.</summary>
    static List<Dictionary<ForEachStatementSyntax, ExpressionSyntax>?> LoopRows(SyntaxNode inv)
    {
        var combos = new List<Dictionary<ForEachStatementSyntax, ExpressionSyntax>> { new() };
        foreach (var loop in inv.Ancestors().OfType<ForEachStatementSyntax>())
        {
            List<ExpressionSyntax>? values = loop.Expression switch
            {
                ArrayCreationExpressionSyntax a => a.Initializer?.Expressions.ToList(),
                ImplicitArrayCreationExpressionSyntax a => a.Initializer.Expressions.ToList(),
                CollectionExpressionSyntax c when c.Elements.All(x => x is ExpressionElementSyntax) =>
                    c.Elements.Cast<ExpressionElementSyntax>().Select(x => x.Expression).ToList(),
                _ => null,
            };
            if (values == null || values.Count == 0 || values.Count * combos.Count > 64)
            {
                continue;
            }
            combos = combos.SelectMany(c => values.Select(v => new Dictionary<ForEachStatementSyntax, ExpressionSyntax>(c) { [loop] = v })).ToList();
        }
        return combos.Cast<Dictionary<ForEachStatementSyntax, ExpressionSyntax>?>().ToList();
    }

    /// <summary>A text part's value. Roslyn leaves the doubled braces of a non-raw <c>@$"</c> string in the
    /// token's ValueText; a raw string has none to undo.</summary>
    static string TextPart(InterpolatedStringExpressionSyntax s, InterpolatedStringTextSyntax t)
    {
        var v = t.TextToken.ValueText;
        return s.StringStartToken.IsKind(SyntaxKind.InterpolatedStringStartToken) ||
               s.StringStartToken.IsKind(SyntaxKind.InterpolatedVerbatimStringStartToken)
            ? v.Replace("{{", "{", StringComparison.Ordinal).Replace("}}", "}", StringComparison.Ordinal)
            : v;
    }

    /// <summary>The manual evaluation of interpolated strings against the compiler's own constants, over every
    /// interpolated string of the tests that the compiler folds.</summary>
    static void CheckInterpolation(Context cx, List<SyntaxTree> trees)
    {
        var (same, differ) = (0, 0);
        foreach (var s in trees.SelectMany(t => t.GetRoot().DescendantNodes().OfType<InterpolatedStringExpressionSyntax>()))
        {
            if (cx.Model(s).GetConstantValue(s) is not { HasValue: true, Value: string c })
            {
                continue;
            }
            var o = new StringBuilder();
            foreach (var part in s.Contents)
            {
                o.Append(part is InterpolatedStringTextSyntax t ? TextPart(s, t) : EvalText(cx, ((InterpolationSyntax)part).Expression));
            }
            if (o.ToString() == c)
            {
                same++;
            }
            else
            {
                differ++;
                var m = o.ToString();
                var at = Enumerable.Range(0, Math.Min(m.Length, c.Length)).FirstOrDefault(i => m[i] != c[i]);
                Console.Error.WriteLine($"interpolation differs: {s.SyntaxTree.FilePath}:{s.GetLocation().GetLineSpan().StartLinePosition.Line + 1} at {at}: [{m.Substring(at, Math.Min(20, m.Length - at))}] vs [{c.Substring(at, Math.Min(20, c.Length - at))}]");
            }
        }
        Console.Error.WriteLine($"interpolation check: {same} equal, {differ} differ");
    }

    static string MemberName(SyntaxNode inv, TypeDeclarationSyntax type)
    {
        foreach (var n in inv.Ancestors())
        {
            if (n == type)
            {
                break;
            }
            switch (n)
            {
                case MethodDeclarationSyntax m when m.Parent == type:
                    return m.Identifier.ValueText;
                case ConstructorDeclarationSyntax c when c.Parent == type:
                    return "ctor";
                case PropertyDeclarationSyntax p when p.Parent == type:
                    return p.Identifier.ValueText;
                case FieldDeclarationSyntax f when f.Parent == type:
                    return f.Declaration.Variables[0].Identifier.ValueText;
            }
        }
        return "member";
    }

    /// <summary>Names: <c>Class/Member</c>, or <c>Class/Member_1</c>, <c>_2</c>, .. when a member has several
    /// inputs; a clash (also one of case only: file systems that ignore case) gets <c>__2</c>, ...</summary>
    static void Name(List<Golden> goldens)
    {
        var used = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var group in goldens.GroupBy(g => (g.Class, g.Member)))
        {
            var items = group.ToList();
            for (var i = 0; i < items.Count; i++)
            {
                var name = items[i].Class + "/" + items[i].Member + (items.Count > 1 ? "_" + (i + 1) : "");
                var unique = name;
                for (var k = 2; !used.Add(unique); k++)
                {
                    unique = name + "__" + k;
                }
                items[i].Name = unique;
            }
        }
        goldens.Sort((x, y) => string.CompareOrdinal(x.Name, y.Name));
    }

    static readonly Dictionary<string, string> s_aliases = new()
    {
        ["source"] = "text",
        ["parseOptions"] = "options",
    };

    static Golden? Extract(Context cx, InvocationExpressionSyntax inv, Call call, string cls, string member)
    {
        // Bind the arguments to the call's parameters.
        var args = new Dictionary<string, ExpressionSyntax>();
        var rest = new List<ExpressionSyntax>();
        var positional = 0;
        var ps = call.Params;
        foreach (var arg in inv.ArgumentList.Arguments)
        {
            if (arg.NameColon != null)
            {
                var n = arg.NameColon.Name.Identifier.ValueText;
                args[s_aliases.GetValueOrDefault(n, n)] = arg.Expression;
                continue;
            }
            var bound = false;
            while (positional < ps.Length && !ps[positional].EndsWith("...", StringComparison.Ordinal))
            {
                var p = ps[positional];
                var optional = p.EndsWith('?');
                var pname = optional ? p[..^1] : s_aliases.GetValueOrDefault(p, p);
                if (optional && pname == "offset" && !arg.Expression.IsKind(SyntaxKind.NumericLiteralExpression))
                {
                    positional++;
                    continue;
                }
                if (optional && pname == "options" && EvalOptions(cx, arg.Expression).Kind == OptKind.NotOptions ||
                    pname == "consumeFullText" && !arg.Expression.IsKind(SyntaxKind.TrueLiteralExpression) &&
                    !arg.Expression.IsKind(SyntaxKind.FalseLiteralExpression))
                {
                    positional = ps.Length;
                    break;
                }
                args[pname] = arg.Expression;
                positional++;
                bound = true;
                break;
            }
            if (!bound)
            {
                rest.Add(arg.Expression);
            }
        }

        if (!args.TryGetValue("text", out var textExpr))
        {
            cx.Skip("no text argument");
            return null;
        }
        var text = EvalText(cx, textExpr);
        if (text == null)
        {
            cx.Skip("text not constant");
            return null;
        }
        if (args.TryGetValue("offset", out var offset) && !IsZero(offset))
        {
            cx.Skip("offset");
            return null;
        }
        if (args.TryGetValue("initialTokenPosition", out var itp) && !IsZero(itp))
        {
            cx.Skip("initial token position");
            return null;
        }
        if (args.TryGetValue("consumeFullText", out var full) && !full.IsKind(SyntaxKind.TrueLiteralExpression))
        {
            cx.Skip("consumeFullText: false");
            return null;
        }

        TestOptions? options = null;
        if (args.TryGetValue("options", out var optExpr))
        {
            var r = EvalOptions(cx, optExpr);
            switch (r.Kind)
            {
                case OptKind.Value:
                    options = r.Value;
                    break;
                case OptKind.Null:
                    break;
                default:
                    cx.Skip("options not evaluated");
                    return null;
            }
        }

        if (call.Wrapper != null)
        {
            options = call.Wrapper(options);
        }
        var entry = call.Entry;
        switch (call.Class)
        {
            case "PreprocessorTests":
            {
                // GetOptions(kind, defines): CSharp4, the given kind, the defines as preprocessor symbols.
                var o = new TestOptions(LanguageVersion.CSharp4, DocumentationMode.Parse, false, false, false);
                var defines = rest;
                if (defines.Count > 0 && EnumMember<SourceCodeKind>(defines[0], "SourceCodeKind") is { } kind)
                {
                    o = o with { Script = kind == SourceCodeKind.Script };
                    defines = defines.Skip(1).ToList();
                }
                else if (defines.Count == 1 && EvalOptions(cx, defines[0]) is { Kind: OptKind.Value } given)
                {
                    o = given.Value;
                    defines = new List<ExpressionSyntax>();
                }
                options = o with { Symbols = o.Symbols || defines.Count > 0 };
                break;
            }
            case "XmlDocCommentTests":
                // GetOptions(defines): CSharp3, Diagnose, the defines as preprocessor symbols.
                options = new TestOptions(LanguageVersion.CSharp3, DocumentationMode.Diagnose, false, rest.Count > 0, false);
                break;
            default:
            {
                var config = s_classes.GetValueOrDefault(cls) ?? s_default;
                if (call.Via == Via.Tree || call.Via == Via.Node && config.Node == null)
                {
                    options = config.Tree(options);
                    if (options == null)
                    {
                        cx.Skip("ParseTree not supported by the class");
                        return null;
                    }
                }
                else if (call.Via == Via.Node)
                {
                    entry = config.Node!.Value;
                    options = config.NodeOptions!(options);
                    if (config.Wrap != null)
                    {
                        text = string.Format(CultureInfo.InvariantCulture, config.Wrap, text);
                    }
                }
                break;
            }
        }
        var opts = options ?? call.Default ?? TestOptions.Regular;
        if (entry == Entry.Name)
        {
            opts = TestOptions.Regular; // SyntaxFactory.ParseName takes no options
        }
        var reason = opts.Script ? "script" : opts.Symbols ? "preprocessor symbols" : opts.Features ? "features" :
            HasLoneSurrogate(text) ? "lone surrogate (not UTF-8)" : null;
        if (reason != null)
        {
            cx.Skip(reason);
            return null;
        }
        var line = inv.GetLocation().GetLineSpan().StartLinePosition.Line + 1;
        return new Golden(cls, member, text, entry, opts.Version, opts.Doc, inv.SyntaxTree.FilePath + ":" + line);
    }

    static bool IsZero(ExpressionSyntax e) => e is LiteralExpressionSyntax { Token.ValueText: "0" };

    static bool HasLoneSurrogate(string s)
    {
        for (var i = 0; i < s.Length; i++)
        {
            if (char.IsHighSurrogate(s[i]) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1]))
            {
                i++;
            }
            else if (char.IsSurrogate(s[i]))
            {
                return true;
            }
        }
        return false;
    }

    // ---- constant text ----

    /// <summary>The text of a string expression the compiler can fold, or a local or field initialized with one
    /// and never assigned again, their concatenation, a non-raw interpolation of such parts, string.Format of
    /// them, SourceText.From(one).</summary>
    static string? EvalText(Context cx, ExpressionSyntax e)
    {
        if (cx.Contains(e.SyntaxTree) && cx.Model(e).GetConstantValue(e) is { HasValue: true } c)
        {
            return c.Value as string;
        }
        switch (e)
        {
            case ParenthesizedExpressionSyntax p:
                return EvalText(cx, p.Expression);
            case BinaryExpressionSyntax b when b.IsKind(SyntaxKind.AddExpression):
            {
                var l = EvalPiece(cx, b.Left);
                var r = l == null ? null : EvalPiece(cx, b.Right);
                return l != null && r != null ? l + r : null;
            }
            case IdentifierNameSyntax id:
                return Initializer(cx, id) is { } init ? EvalText(cx, init) : null;
            case ConditionalExpressionSyntax q when EvalBool(cx, q.Condition) is { } b:
                return EvalText(cx, b ? q.WhenTrue : q.WhenFalse);
            case InterpolatedStringExpressionSyntax s:
            {
                var o = new StringBuilder();
                foreach (var part in s.Contents)
                {
                    switch (part)
                    {
                        case InterpolatedStringTextSyntax t:
                            o.Append(TextPart(s, t));
                            break;
                        case InterpolationSyntax { AlignmentClause: null, FormatClause: null } i when EvalPiece(cx, i.Expression) is { } v:
                            o.Append(v);
                            break;
                        default:
                            return null;
                    }
                }
                return o.ToString();
            }
            case InvocationExpressionSyntax { Expression: MemberAccessExpressionSyntax ma } inv
                when ma.Name.Identifier.ValueText == "GetText" && ma.Expression.ToString() == "SyntaxFacts" &&
                     inv.ArgumentList.Arguments.Count == 1:
                return EvalKind(cx, inv.ArgumentList.Arguments[0].Expression) is { } kind ? SyntaxFacts.GetText(kind) : null;
            case InvocationExpressionSyntax { Expression: MemberAccessExpressionSyntax ma } inv
                when ma.Name.Identifier.ValueText == "From" && ma.Expression.ToString() == "SourceText" &&
                     inv.ArgumentList.Arguments.Count == 1:
                return EvalText(cx, inv.ArgumentList.Arguments[0].Expression);
            case InvocationExpressionSyntax { Expression: MemberAccessExpressionSyntax ma } inv
                when ma.Name.Identifier.ValueText == "Format" && ma.Expression.ToString() is "string" or "String" &&
                     inv.ArgumentList.Arguments.Count >= 1:
            {
                var parts = inv.ArgumentList.Arguments.Select(x => EvalText(cx, x.Expression)).ToList();
                return parts.Any(p => p == null)
                    ? null
                    : string.Format(CultureInfo.InvariantCulture, parts[0]!, parts.Skip(1).Cast<object>().ToArray());
            }
        }
        return null;
    }

    /// <summary>A string, or a char or number constant as the string it adds to a concatenation.</summary>
    static string? EvalPiece(Context cx, ExpressionSyntax e)
    {
        if (EvalText(cx, e) is { } s)
        {
            return s;
        }
        var x = e is IdentifierNameSyntax id && Initializer(cx, id) is { } init ? init : e;
        return cx.Contains(x.SyntaxTree) && cx.Model(x).GetConstantValue(x) is { HasValue: true, Value: char or int or long } c
            ? Convert.ToString(c.Value, CultureInfo.InvariantCulture)
            : null;
    }

    /// <summary>The initializer of the local, field or property <paramref name="id"/> names when nothing assigns it
    /// again (a field without one: its single assignment in a constructor).</summary>
    static ExpressionSyntax? Initializer(Context cx, IdentifierNameSyntax id)
    {
        if (!cx.Contains(id.SyntaxTree))
        {
            return null;
        }
        var symbol = cx.Model(id).GetSymbolInfo(id).Symbol;
        if (symbol is IParameterSymbol { ContainingSymbol: IMethodSymbol m } parameter && cx.Row != null &&
            m.DeclaringSyntaxReferences.FirstOrDefault()?.GetSyntax() == cx.Method)
        {
            return cx.Row.GetValueOrDefault(parameter.Name);
        }
        if (symbol is not (ILocalSymbol or IFieldSymbol or IPropertySymbol))
        {
            return null;
        }
        var decl = symbol.DeclaringSyntaxReferences.FirstOrDefault()?.GetSyntax();
        if (decl is ForEachStatementSyntax loop)
        {
            return cx.Loop?.GetValueOrDefault(loop);
        }
        var init = decl switch
        {
            VariableDeclaratorSyntax v => v.Initializer?.Value,
            PropertyDeclarationSyntax p => p.ExpressionBody?.Expression ?? p.Initializer?.Value,
            _ => null,
        };
        SyntaxNode? scope = symbol is ILocalSymbol
            ? decl?.Ancestors().FirstOrDefault(n => n is BaseMethodDeclarationSyntax or LocalFunctionStatementSyntax or
                AccessorDeclarationSyntax or BasePropertyDeclarationSyntax)
            : decl?.Ancestors().OfType<TypeDeclarationSyntax>().FirstOrDefault();
        if (scope == null)
        {
            return null;
        }
        if (symbol is ILocalSymbol)
        {
            return LastAssignment(cx, id, symbol, decl, init, scope);
        }
        var assignments = new List<ExpressionSyntax>();
        foreach (var n in scope.DescendantNodes())
        {
            var target = n switch
            {
                AssignmentExpressionSyntax a => a.Left,
                ArgumentSyntax { RefKindKeyword.RawKind: not 0 } arg => arg.Expression,
                _ => null,
            };
            if (target is not IdentifierNameSyntax t || t.Identifier.ValueText != symbol.Name ||
                !SymbolEqualityComparer.Default.Equals(cx.Model(t).GetSymbolInfo(t).Symbol, symbol))
            {
                continue;
            }
            if (n is AssignmentExpressionSyntax simple && simple.IsKind(SyntaxKind.SimpleAssignmentExpression) &&
                symbol is IFieldSymbol && init == null && n.Ancestors().OfType<ConstructorDeclarationSyntax>().Any())
            {
                assignments.Add(simple.Right);
                continue;
            }
            return null;
        }
        return init ?? (assignments.Count == 1 ? assignments[0] : null);
    }

    /// <summary>A local's value at <paramref name="use"/>: its initializer or the last plain assignment before the
    /// use, when that is a statement of a block enclosing the use and nothing else (a ref, a compound assignment,
    /// an assignment in a nested statement or a lambda) writes it in between.</summary>
    static ExpressionSyntax? LastAssignment(Context cx, IdentifierNameSyntax use, ISymbol symbol, SyntaxNode? decl,
        ExpressionSyntax? init, SyntaxNode scope)
    {
        var value = init;
        var valuePos = decl?.SpanStart ?? -1;
        foreach (var n in scope.DescendantNodes())
        {
            var target = n switch
            {
                AssignmentExpressionSyntax a => a.Left,
                ArgumentSyntax { RefKindKeyword.RawKind: not 0 } arg => arg.Expression,
                PrefixUnaryExpressionSyntax or PostfixUnaryExpressionSyntax => null,
                _ => null,
            };
            if (target is not IdentifierNameSyntax t || t.Identifier.ValueText != symbol.Name ||
                !SymbolEqualityComparer.Default.Equals(cx.Model(t).GetSymbolInfo(t).Symbol, symbol))
            {
                continue;
            }
            if (n.SpanStart >= use.SpanStart)
            {
                if (n.Span.Contains(use.Span))
                {
                    return null; // the use is inside the assignment (x = x + ...)
                }
                continue;
            }
            if (n is AssignmentExpressionSyntax simple && simple.IsKind(SyntaxKind.SimpleAssignmentExpression) &&
                n.Parent is ExpressionStatementSyntax { Parent: BlockSyntax block } && block.Span.Contains(use.Span) &&
                !n.Ancestors().TakeWhile(x => x != block).Any(x => x is AnonymousFunctionExpressionSyntax))
            {
                if (n.SpanStart > valuePos)
                {
                    value = simple.Right;
                    valuePos = n.SpanStart;
                }
                continue;
            }
            return null;
        }
        return value;
    }

    // ---- options ----

    enum OptKind { Value, Null, NotOptions, Unknown }

    readonly record struct OptResult(OptKind Kind, TestOptions Value)
    {
        public static readonly OptResult Unknown = new(OptKind.Unknown, default);
        public static readonly OptResult NotOptions = new(OptKind.NotOptions, default);
        public static readonly OptResult Null = new(OptKind.Null, default);
        public static OptResult Of(TestOptions o) => new(OptKind.Value, o);
    }

    /// <summary>Whether a call chain starts at a name with <c>Diagnostic</c> in it: <c>Diagnostic(ErrorCode.X, "y")</c>
    /// <c>.WithLocation(1, 2)</c>, an expected diagnostic.</summary>
    static bool MentionsDiagnostics(ExpressionSyntax e)
    {
        for (var x = e; ;)
        {
            switch (x)
            {
                case InvocationExpressionSyntax inv:
                    x = inv.Expression;
                    continue;
                case MemberAccessExpressionSyntax ma:
                    x = ma.Expression;
                    continue;
                case IdentifierNameSyntax id:
                    return id.Identifier.ValueText.Contains("Diagnostic", StringComparison.Ordinal);
                default:
                    return false;
            }
        }
    }

    static OptResult EvalOptions(Context cx, ExpressionSyntax e)
    {
        switch (e)
        {
            case LiteralExpressionSyntax l when l.IsKind(SyntaxKind.NullLiteralExpression) || l.IsKind(SyntaxKind.DefaultLiteralExpression):
                return OptResult.Null;
            case ParenthesizedExpressionSyntax p:
                return EvalOptions(cx, p.Expression);
            case PostfixUnaryExpressionSyntax pu when pu.IsKind(SyntaxKind.SuppressNullableWarningExpression):
                return EvalOptions(cx, pu.Operand);
            case ArrayCreationExpressionSyntax or ImplicitArrayCreationExpressionSyntax or CollectionExpressionSyntax:
            case LiteralExpressionSyntax:
            case PrefixUnaryExpressionSyntax { Operand: LiteralExpressionSyntax }:
                return OptResult.NotOptions;
            case ConditionalExpressionSyntax q:
                return EvalBool(cx, q.Condition) is { } b ? EvalOptions(cx, b ? q.WhenTrue : q.WhenFalse) : OptResult.Unknown;
            case MemberAccessExpressionSyntax ma when ma.Expression.ToString() == "TestOptions":
                return TestOptions.Named(ma.Name.Identifier.ValueText) is { } named ? OptResult.Of(named) : OptResult.Unknown;
            case MemberAccessExpressionSyntax ma when ma.ToString() == "CSharpParseOptions.Default":
                return OptResult.Of(TestOptions.Regular);
            case InvocationExpressionSyntax inv when MentionsDiagnostics(inv):
                return OptResult.NotOptions;
            case InvocationExpressionSyntax { Expression: MemberAccessExpressionSyntax ma } inv:
            {
                var recv = EvalOptions(cx, ma.Expression);
                if (recv.Kind != OptKind.Value)
                {
                    return recv.Kind == OptKind.NotOptions ? recv : OptResult.Unknown;
                }
                var o = recv.Value;
                var a = inv.ArgumentList.Arguments;
                var method = ma.Name.Identifier.ValueText;
                switch (method)
                {
                    case "WithLanguageVersion" when a.Count == 1:
                        return EvalVersion(cx, a[0].Expression) is { } v ? OptResult.Of(o.WithVersion(v)) : OptResult.Unknown;
                    case "WithDocumentationMode" when a.Count == 1:
                        return EnumMember<DocumentationMode>(a[0].Expression, "DocumentationMode") is { } d
                            ? OptResult.Of(o with { Doc = d })
                            : OptResult.Unknown;
                    case "WithKind" when a.Count == 1:
                        return EnumMember<SourceCodeKind>(a[0].Expression, "SourceCodeKind") is { } k
                            ? OptResult.Of(o with { Script = k == SourceCodeKind.Script })
                            : OptResult.Unknown;
                    case "WithPreprocessorSymbols":
                        return OptResult.Of(o with { Symbols = a.Count > 0 && !(a.Count == 1 && IsEmptyCollection(a[0].Expression)) });
                }
                if (method.StartsWith("With", StringComparison.Ordinal) &&
                    (method.Contains("Feature", StringComparison.Ordinal) || method is "WithExperimental" or "WithNullablePublicOnly" or
                        "WithNoRefSafetyRulesAttribute" or "WithDisableLengthBasedSwitch"))
                {
                    return OptResult.Of(o with { Features = true });
                }
                return OptResult.Unknown;
            }
            case ObjectCreationExpressionSyntax oc:
                return oc.Type.ToString() == "CSharpParseOptions" ? NewOptions(cx, oc.ArgumentList) : OptResult.NotOptions;
            case IdentifierNameSyntax id:
            {
                if (Initializer(cx, id) is { } init)
                {
                    return EvalOptions(cx, init);
                }
                var name = id.Identifier.ValueText;
                if (name.Contains("ption", StringComparison.Ordinal))
                {
                    return OptResult.Unknown;
                }
                return name.Contains("iagnostic", StringComparison.Ordinal) || name.Contains("rror", StringComparison.Ordinal) ||
                       name.StartsWith("expected", StringComparison.OrdinalIgnoreCase)
                    ? OptResult.NotOptions
                    : OptResult.Unknown;
            }
            default:
                return OptResult.Unknown;
        }
    }

    /// <summary><c>new CSharpParseOptions(languageVersion, documentationMode, kind, preprocessorSymbols)</c>.</summary>
    static OptResult NewOptions(Context cx, ArgumentListSyntax? args)
    {
        var o = TestOptions.Regular;
        string[] order = { "languageVersion", "documentationMode", "kind", "preprocessorSymbols" };
        var i = 0;
        foreach (var arg in args?.Arguments ?? default)
        {
            var n = arg.NameColon?.Name.Identifier.ValueText ?? (i < order.Length ? order[i++] : "");
            switch (n)
            {
                case "languageVersion" when EvalVersion(cx, arg.Expression) is { } v:
                    o = o.WithVersion(v);
                    break;
                case "documentationMode" when EnumMember<DocumentationMode>(arg.Expression, "DocumentationMode") is { } d:
                    o = o with { Doc = d };
                    break;
                case "kind" when EnumMember<SourceCodeKind>(arg.Expression, "SourceCodeKind") is { } k:
                    o = o with { Script = k == SourceCodeKind.Script };
                    break;
                case "preprocessorSymbols":
                    o = o with
                    {
                        Symbols = !arg.Expression.IsKind(SyntaxKind.NullLiteralExpression) && !IsEmptyCollection(arg.Expression),
                    };
                    break;
                default:
                    return OptResult.Unknown;
            }
        }
        return OptResult.Of(o);
    }

    static bool IsEmptyCollection(ExpressionSyntax e) => e switch
    {
        CollectionExpressionSyntax c => c.Elements.Count == 0,
        ArrayCreationExpressionSyntax a => a.Initializer is { Expressions.Count: 0 } ||
                                           a.Initializer == null && a.Type.RankSpecifiers.Count == 1 &&
                                           a.Type.RankSpecifiers[0].Sizes is [LiteralExpressionSyntax { Token.ValueText: "0" }],
        _ => e.ToString() is "Array.Empty<string>()",
    };

    static T? EnumMember<T>(ExpressionSyntax e, string type) where T : struct, Enum =>
        e is MemberAccessExpressionSyntax ma && ma.Expression.ToString() == type &&
        Enum.TryParse<T>(ma.Name.Identifier.ValueText, out var v) ? v : null;

    static SyntaxKind? EvalKind(Context cx, ExpressionSyntax e) => e switch
    {
        MemberAccessExpressionSyntax => EnumMember<SyntaxKind>(e, "SyntaxKind"),
        IdentifierNameSyntax id when Initializer(cx, id) is { } init => EvalKind(cx, init),
        _ => null,
    };

    static bool? EvalBool(Context cx, ExpressionSyntax e) => e switch
    {
        LiteralExpressionSyntax l when l.IsKind(SyntaxKind.TrueLiteralExpression) => true,
        LiteralExpressionSyntax l when l.IsKind(SyntaxKind.FalseLiteralExpression) => false,
        PrefixUnaryExpressionSyntax { RawKind: (int)SyntaxKind.LogicalNotExpression } n => !EvalBool(cx, n.Operand),
        ParenthesizedExpressionSyntax p => EvalBool(cx, p.Expression),
        IdentifierNameSyntax id when Initializer(cx, id) is { } init => EvalBool(cx, init),
        _ => null,
    };

    static LanguageVersion? EvalVersion(Context cx, ExpressionSyntax e) => e switch
    {
        ConditionalExpressionSyntax q when EvalBool(cx, q.Condition) is { } b => EvalVersion(cx, b ? q.WhenTrue : q.WhenFalse),
        MemberAccessExpressionSyntax ma when ma.ToString() == "LanguageVersionFacts.CSharpNext" => LanguageVersion.Preview,
        MemberAccessExpressionSyntax => EnumMember<LanguageVersion>(e, "LanguageVersion"),
        IdentifierNameSyntax id when Initializer(cx, id) is { } init => EvalVersion(cx, init),
        _ => null,
    };
}
