// The getters mode (README.md, "Getters"): Roslyn's answer of every syntax node property crates/csharp-syntax's
// syntax_getters ports, on every node, as `ksdump --lang csharp --getters` prints ours.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace CSharpOracle;

/// <summary>The lines of one tree: <c>&lt;id&gt; &lt;getter&gt; &lt;answer&gt;</c>, nodes in preorder. Every element
/// of the dump (node, token, trivia, structured trivia) is numbered in the order the dump prints it, which is the
/// port's node ids; a structured trivia and its structure node share one number. Per node: <c>Span</c> (UTF-8 offsets),
/// its fields and those of its abstract classes (<c>Fields.g.cs</c>), the computed properties
/// (<c>Computed</c>). An answer is an id, <c>-</c> for a null node or a <c>default</c> token, ids joined by <c>,</c>
/// (<c>.</c> when empty), a count, <c>true</c> or <c>false</c>, a quoted text escaped as the dump's, or <c>!</c> where
/// Roslyn throws.</summary>
sealed class GetterLines
{
    readonly StringBuilder _out = new();
    readonly int[] _utf8;
    readonly Dictionary<SyntaxNode, int> _nodes = new(ReferenceEqualityComparer.Instance);
    readonly Dictionary<SyntaxToken, int> _tokens = new();
    readonly List<SyntaxNode> _order = new();
    int _id;

    GetterLines(string text)
    {
        _utf8 = Printer.Utf8Offsets(text);
    }

    public override string ToString() => _out.ToString();

    /// <summary>The lines of the tree under <paramref name="root"/>.</summary>
    public static string Of(string text, SyntaxNode root)
    {
        var g = new GetterLines(text);
        g.Number(root);
        foreach (var node in g._order)
        {
            g._id = g._nodes[node];
            var span = node.Span;
            g.Raw("Span", g._utf8[span.Start] + ".." + g._utf8[span.End]);
            Fields.Print(node, g);
            Computed(node, g);
            if (g._out.Length > Printer.MaxChars)
            {
                throw new DumpTooLargeException();
            }
        }
        return g.ToString();
    }

    /// <summary>Numbers every element as <c>Printer.Drain</c> prints it (and so as the port's ids run).</summary>
    void Number(SyntaxNode root)
    {
        var next = 0;
        var stack = new Stack<(SyntaxNode? Node, SyntaxToken Token)>();
        stack.Push((root, default));
        var children = new List<SyntaxNodeOrToken>();
        void PushChildren(SyntaxNode node)
        {
            children.Clear();
            foreach (var c in node.ChildNodesAndTokens())
            {
                children.Add(c);
            }
            for (var i = children.Count - 1; i >= 0; i--)
            {
                stack.Push(children[i].IsNode ? (children[i].AsNode(), default) : (null, children[i].AsToken()));
            }
        }
        while (stack.Count > 0)
        {
            var (node, token) = stack.Pop();
            if (node != null)
            {
                _nodes[node] = next++;
                _order.Add(node);
                PushChildren(node);
                continue;
            }
            // a token: its leading trivia, itself, its trailing trivia (a structure's elements after its trivia)
            foreach (var t in token.LeadingTrivia)
            {
                NumberTrivia(t, ref next);
            }
            _tokens[token] = next++;
            foreach (var t in token.TrailingTrivia)
            {
                NumberTrivia(t, ref next);
            }
        }
    }

    /// <summary>A trivia's number; a structured trivia's structure node takes it, and its elements follow.</summary>
    void NumberTrivia(SyntaxTrivia trivia, ref int next)
    {
        if (!trivia.HasStructure)
        {
            next++;
            return;
        }
        var structure = trivia.GetStructure()!;
        // the structure's own preorder: its root shares the trivia's number
        var stack = new Stack<(SyntaxNode? Node, SyntaxToken Token)>();
        stack.Push((structure, default));
        var children = new List<SyntaxNodeOrToken>();
        while (stack.Count > 0)
        {
            var (node, token) = stack.Pop();
            if (node != null)
            {
                _nodes[node] = next++;
                _order.Add(node);
                children.Clear();
                foreach (var c in node.ChildNodesAndTokens())
                {
                    children.Add(c);
                }
                for (var i = children.Count - 1; i >= 0; i--)
                {
                    stack.Push(children[i].IsNode ? (children[i].AsNode(), default) : (null, children[i].AsToken()));
                }
                continue;
            }
            foreach (var t in token.LeadingTrivia)
            {
                NumberTrivia(t, ref next);
            }
            _tokens[token] = next++;
            foreach (var t in token.TrailingTrivia)
            {
                NumberTrivia(t, ref next);
            }
        }
    }

    void Raw(string getter, string answer)
    {
        _out.Append(_id).Append(' ').Append(getter).Append(' ').Append(answer).Append('\n');
    }

    string IdOf(SyntaxNode? node) => node == null ? "-" : _nodes.TryGetValue(node, out var id) ? id.ToString(CultureInfo.InvariantCulture) : "?";

    string IdOf(SyntaxToken token) =>
        token.RawKind == 0 ? "-" : _tokens.TryGetValue(token, out var id) ? id.ToString(CultureInfo.InvariantCulture) : "?";

    public void Token(string getter, SyntaxToken token) => Raw(getter, IdOf(token));

    public void Node(string getter, SyntaxNode? node) => Raw(getter, IdOf(node));

    public void List<T>(string getter, SyntaxList<T> list) where T : SyntaxNode => Ids(getter, list.Select(n => IdOf(n)));

    public void Separated<T>(string getter, SeparatedSyntaxList<T> list) where T : SyntaxNode =>
        Ids(getter, list.Select(n => IdOf(n)));

    public void Tokens(string getter, SyntaxTokenList list) => Ids(getter, list.Select(t => IdOf(t)));

    void Ids(string getter, IEnumerable<string> ids)
    {
        var joined = string.Join(',', ids);
        Raw(getter, joined.Length == 0 ? "." : joined);
    }

    void Count(string getter, int value) => Raw(getter, value.ToString(CultureInfo.InvariantCulture));

    void Bool(string getter, bool value) => Raw(getter, value ? "true" : "false");

    void Text(string getter, string? text)
    {
        if (text == null)
        {
            Raw(getter, "-");
            return;
        }
        var o = new StringBuilder("'");
        Printer.Escape(o, text);
        Raw(getter, o.Append('\'').ToString());
    }

    /// <summary>One getter, <c>!</c> where Roslyn throws.</summary>
    void Try(string getter, Action<string> print)
    {
        try
        {
            print(getter);
        }
        catch (Exception e) when (e is not DumpTooLargeException)
        {
            Raw(getter, "!");
        }
    }

    static readonly BindingFlags s_internal = BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public;
    static readonly MethodInfo s_getUnqualifiedName = typeof(NameSyntax).GetMethod("GetUnqualifiedName", s_internal)!;
    static readonly MethodInfo s_getAliasQualifierOpt = typeof(NameSyntax).GetMethod("GetAliasQualifierOpt", s_internal)!;
    static readonly MethodInfo s_errorDisplayName = typeof(NameSyntax).GetMethod("ErrorDisplayName", s_internal)!;
    static readonly PropertyInfo s_primaryConstructorBaseTypeIfClass =
        typeof(TypeDeclarationSyntax).GetProperty("PrimaryConstructorBaseTypeIfClass", s_internal)!;
    static readonly PropertyInfo s_isArgList = typeof(ParameterSyntax).GetProperty("IsArgList", s_internal)!;
    static readonly PropertyInfo s_parameterCount = typeof(ParameterListSyntax).GetProperty("ParameterCount", s_internal)!;

    static object? Call(MethodInfo method, object target)
    {
        try
        {
            return method.Invoke(target, null);
        }
        catch (TargetInvocationException e) when (e.InnerException != null)
        {
            throw e.InnerException;
        }
    }

    static object? Get(PropertyInfo property, object target)
    {
        try
        {
            return property.GetValue(target);
        }
        catch (TargetInvocationException e) when (e.InnerException != null)
        {
            throw e.InnerException;
        }
    }

    /// <summary>The computed properties of the partial classes, in the order of crates/csharp-syntax's
    /// syntax_getters::getters::computed; the internal ones by reflection.</summary>
    static void Computed(SyntaxNode node, GetterLines o)
    {
        if (node is NameSyntax name)
        {
            o.Try("NameSyntax.Arity", g => o.Count(g, name.Arity));
            o.Try("NameSyntax.GetUnqualifiedName", g => o.Node(g, (SyntaxNode?)Call(s_getUnqualifiedName, name)));
            o.Try("NameSyntax.GetAliasQualifierOpt", g => o.Text(g, (string?)Call(s_getAliasQualifierOpt, name)));
            o.Try("NameSyntax.ErrorDisplayName", g => o.Text(g, (string?)Call(s_errorDisplayName, name)));
        }
        if (node is GenericNameSyntax generic)
        {
            o.Try("GenericNameSyntax.IsUnboundGenericName", g => o.Bool(g, generic.IsUnboundGenericName));
        }
        if (node is TypeDeclarationSyntax type)
        {
            o.Try("TypeDeclarationSyntax.Arity", g => o.Count(g, type.Arity));
            o.Try("TypeDeclarationSyntax.PrimaryConstructorBaseTypeIfClass",
                g => o.Node(g, (SyntaxNode?)Get(s_primaryConstructorBaseTypeIfClass, type)));
        }
        if (node is MethodDeclarationSyntax method)
        {
            o.Try("MethodDeclarationSyntax.Arity", g => o.Count(g, method.Arity));
        }
        if (node is DelegateDeclarationSyntax @delegate)
        {
            o.Try("DelegateDeclarationSyntax.Arity", g => o.Count(g, @delegate.Arity));
        }
        if (node is ParameterSyntax parameter)
        {
            o.Try("ParameterSyntax.IsArgList", g => o.Bool(g, (bool)Get(s_isArgList, parameter)!));
        }
        if (node is ParameterListSyntax parameters)
        {
            o.Try("ParameterListSyntax.ParameterCount", g => o.Count(g, (int)Get(s_parameterCount, parameters)!));
        }
        if (node is ArrayRankSpecifierSyntax rank)
        {
            o.Try("ArrayRankSpecifierSyntax.Rank", g => o.Count(g, rank.Rank));
        }
        if (node is AnonymousFunctionExpressionSyntax function)
        {
            o.Try("AnonymousFunctionExpressionSyntax.Body", g => o.Node(g, function.Body));
            o.Try("AnonymousFunctionExpressionSyntax.AsyncKeyword", g => o.Token(g, function.AsyncKeyword));
        }
        if (node is LocalDeclarationStatementSyntax local)
        {
            o.Try("LocalDeclarationStatementSyntax.IsConst", g => o.Bool(g, local.IsConst));
        }
        if (node is UsingDirectiveSyntax usingDirective)
        {
            o.Try("UsingDirectiveSyntax.Name", g => o.Node(g, usingDirective.Name));
        }
        if (node is SubpatternSyntax subpattern)
        {
            o.Try("SubpatternSyntax.NameColon", g => o.Node(g, subpattern.NameColon));
        }
        if (node is TypeSyntax typeSyntax)
        {
            o.Try("TypeSyntax.IsVar", g => o.Bool(g, typeSyntax.IsVar));
            o.Try("TypeSyntax.IsUnmanaged", g => o.Bool(g, typeSyntax.IsUnmanaged));
            o.Try("TypeSyntax.IsNotNull", g => o.Bool(g, typeSyntax.IsNotNull));
            o.Try("TypeSyntax.IsNint", g => o.Bool(g, typeSyntax.IsNint));
            o.Try("TypeSyntax.IsNuint", g => o.Bool(g, typeSyntax.IsNuint));
        }
        if (node is DirectiveTriviaSyntax directive)
        {
            o.Try("DirectiveTriviaSyntax.DirectiveNameToken", g => o.Token(g, directive.DirectiveNameToken));
            o.Try("DirectiveTriviaSyntax.IsActive", g => o.Bool(g, directive.IsActive));
        }
        if (node is BranchingDirectiveTriviaSyntax branching)
        {
            o.Try("BranchingDirectiveTriviaSyntax.BranchTaken", g => o.Bool(g, branching.BranchTaken));
        }
        if (node is ConditionalDirectiveTriviaSyntax conditional)
        {
            o.Try("ConditionalDirectiveTriviaSyntax.ConditionValue", g => o.Bool(g, conditional.ConditionValue));
        }
    }
}

static class GettersMode
{
    static readonly UTF8Encoding s_utf8 = new(false);

    /// <summary>Per list line <c>PATH&lt;TAB&gt;LINES&lt;TAB&gt;HASH</c> (the FNV-1a 64 of the lines' UTF-8 bytes), or
    /// <c>PATH&lt;TAB&gt;failure&lt;TAB&gt;REASON</c>; with <c>--full OUT</c> the lines go to
    /// <c>OUT/&lt;path&gt;.getters.txt</c>.</summary>
    public static int Run(Args a)
    {
        var defaults = Options.FromArgs(a);
        var entry = Options.ParseEntry(a.Get("entry") ?? "");
        var jobs = Job.ReadList(a.Need("list"), entry, defaults);
        var root = a.Need("root");
        var full = a.Get("full");
        var lines = new string[jobs.Count];
        Workers.Run(jobs.Count, a.Threads, a.StackBytes, i =>
        {
            var job = jobs[i];
            string line;
            try
            {
                var text = Dumper.Decode(File.ReadAllBytes(Path.Combine(root, job.Path)));
                if (text == null)
                {
                    line = job.Path + "\tfailure\tinvalid UTF-8";
                }
                else if (job.Entry == Entry.Tokens)
                {
                    // lexed tokens: no nodes, no lines
                    line = job.Path + "\t0\t" + Dumper.Fnv([]);
                }
                else if (Root(text, job) is not { } node)
                {
                    line = job.Path + "\tfailure\tno " + Options.EntryName(job.Entry);
                }
                else
                {
                    var bytes = s_utf8.GetBytes(GetterLines.Of(text, node));
                    var count = bytes.Count(b => b == (byte)'\n');
                    line = job.Path + "\t" + count + "\t" + Dumper.Fnv(bytes);
                    if (full != null)
                    {
                        var target = Path.Combine(full, job.Path + ".getters.txt");
                        Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                        Files.Replace(target, bytes);
                    }
                }
            }
            catch (DumpTooLargeException e)
            {
                line = job.Path + "\tfailure\t" + e.Message;
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                line = job.Path + "\tfailure\t" + e.GetType().Name + ": " + e.Message.Replace('\n', ' ').Replace('\t', ' ');
            }
            lines[i] = line;
        });
        using var stdout = Console.OpenStandardOutput();
        var o = new StringBuilder();
        foreach (var l in lines)
        {
            o.Append(l).Append('\n');
        }
        stdout.Write(s_utf8.GetBytes(o.ToString()));
        return 0;
    }

    /// <summary>The root the dump of the job would print (<c>Dumper.Dump</c>'s entries); null for <c>tokens</c>, whose
    /// dump has no nodes, and where the entry returns none.</summary>
    static SyntaxNode? Root(string text, Job job)
    {
        var po = job.Options.Parse;
        return job.Entry switch
        {
            Entry.CompilationUnit => CSharpSyntaxTree.ParseText(text, po).GetRoot(),
            Entry.Expression => SyntaxFactory.ParseExpression(text, 0, po, consumeFullText: true),
            Entry.Statement => SyntaxFactory.ParseStatement(text, 0, po, consumeFullText: true),
            Entry.Member => SyntaxFactory.ParseMemberDeclaration(text, 0, po, consumeFullText: true),
            Entry.TypeName => SyntaxFactory.ParseTypeName(text, 0, po, consumeFullText: true),
            Entry.Name => SyntaxFactory.ParseName(text, 0, consumeFullText: true),
            Entry.ParameterList => SyntaxFactory.ParseParameterList(text, 0, po, consumeFullText: true),
            Entry.BracketedParameterList => SyntaxFactory.ParseBracketedParameterList(text, 0, po, consumeFullText: true),
            Entry.ArgumentList => SyntaxFactory.ParseArgumentList(text, 0, po, consumeFullText: true),
            Entry.BracketedArgumentList => SyntaxFactory.ParseBracketedArgumentList(text, 0, po, consumeFullText: true),
            Entry.AttributeArgumentList => SyntaxFactory.ParseAttributeArgumentList(text, 0, po, consumeFullText: true),
            _ => null,
        };
    }
}
