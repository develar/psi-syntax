// Batch Java PSI oracle: parses each listed file with IntelliJ's Java parser (java-syntax on the
// platform syntax builder, as the IDE runs it) and writes the PSI dump the Java parsing tests
// compare against, plus one JSONL index line per file.
//
// Run as a single-file source program on the IDE's jars (see tools/java-oracle/README.md):
//
//   java -Xss1g -cp "$LIB/*" tools/java-oracle/JavaPsiOracle.java \
//     --list files.txt --root DIR --out DUMP_DIR [--level JDK_28] [--trim] [--node]
//
// The dump is DebugUtil.psiToString(file, showWhitespaces=true, showRanges=false), trimmed, exactly
// as ParsingTestFixture.checkResult prints it for community/java/java-tests/testData/psi/**/*.txt.
// --node also writes DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true), trimmed: the
// `*_node.txt` goldens of AbstractBasicJavaParsingTestCase.
//
// A list line is "PATH[<TAB>LEVEL[<TAB>ENTRY[<TAB>TRIM[<TAB>...]]]]", so testdata/java/manifest.tsv
// is a list. LEVEL is a LanguageLevel constant (JDK_1_8, JDK_X); without it the file is parsed at
// --level, LanguageLevel.HIGHEST by default. The tests set the level as the file's
// LanguageLevelKey.FILE_LANGUAGE_LEVEL_KEY; so does the oracle. TRIM 1 or 0 overrides --trim.
// ENTRY is `file` (default: a PsiJavaFile through JavaParserDefinition, as the parser-full tests
// and the IDE parse) or one parser entry of the parser-partial tests, run as
// JavaParsingTestConfiguratorBase runs it: a PsiJavaFileImpl whose root is the IFileElementType
// "test.java.file", parsed by `root = mark(); ENTRY; if (!eof) error "Unparsed tokens" over the rest;
// root.done(test.java.file)` on a debug-mode builder:
//   annotations            DeclarationParser.parseAnnotations(builder)
//   class_body             DeclarationParser.parseClassBodyWithBraces(builder, false, false)
//   class_body_annotation  DeclarationParser.parseClassBodyWithBraces(builder, true, false)
//   class_body_enum        DeclarationParser.parseClassBodyWithBraces(builder, false, true)
//   expression             ExpressionParser.parse(builder)
//   file_parser            FileParser.parse(builder)
//   pattern                PatternParser.isPattern(builder) (must hold), then parsePattern(builder)
//   reference              ReferenceParser.parseJavaCodeReference(builder, false, false, false, false)
//   reference_incomplete   ReferenceParser.parseJavaCodeReference(builder, true, false, false, false)
//   type                   ReferenceParser.parseType(builder, ELLIPSIS | DIAMONDS | DISJUNCTIONS)
//   type_params            ReferenceParser.parseTypeParameters(builder)
//   code_block_deep        StatementParser.parseCodeBlockDeep(builder, true)
//   statements             StatementParser.parseStatements(builder)
//
// Text: the file is decoded as UTF-8 (a malformed file is a failure), line separators are converted
// to '\n' as the IDE does when it loads a file, and --trim also trims it (chars <= ' ') as
// ParsingTestFixture.loadFile does for a golden. The PSI file is named after the source file.
//
// Index fields: path (relative to --root), dump (relative to --out), level, entry, psiErrors,
// crlf, firstError {offset, line, column, message} or null, failure (on a crash).
//
// --serve keeps one JVM for many batches: each stdin line "LIST<TAB>ROOT<TAB>OUT" is parsed like a
// --list run, then "done" is printed on stdout.
//
// --getters writes OUT/<path>.getters.txt instead of the dumps: the answers of the PSI getters that
// java_syntax::psi ports, asked of the real PSI, one line "<id> <getter> <answer>" per getter of each node
// as `ksdump --getters` prints ours (crates/java-syntax/src/psi/getters.rs has the format). <id> is the
// node's index in a preorder walk of file.getNode() (whitespace, comments and doc comment internals
// included: psi-syntax's NodeId, the node dumps being equal). An answer that IntelliJ throws on is `!`.
// Errors IntelliJ only logs (LOG.error, LOG.assertTrue) are not thrown, as in the IDE, and Java asserts
// are off; PsiAugmentProvider's extension point is registered empty, so no augment applies.

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

import com.intellij.core.CoreApplicationEnvironment;
import com.intellij.java.syntax.parser.JavaParser;
import com.intellij.java.syntax.parser.PatternParser;
import com.intellij.java.syntax.parser.ReferenceParser;
import com.intellij.lang.ASTNode;
import com.intellij.lang.java.parser.JavaParserUtil;
import com.intellij.platform.syntax.SyntaxElementType;
import com.intellij.platform.syntax.parser.SyntaxTreeBuilder;
import com.intellij.platform.syntax.psi.ElementTypeConverter;
import com.intellij.platform.syntax.psi.ElementTypeConverterFactory;
import com.intellij.platform.syntax.psi.ElementTypeConverterKt;
import com.intellij.platform.syntax.psi.PsiSyntaxBuilder;
import com.intellij.openapi.project.Project;
import com.intellij.psi.FileViewProvider;
import com.intellij.psi.PsiManager;
import com.intellij.psi.SingleRootFileViewProvider;
import com.intellij.psi.impl.source.PsiJavaFileImpl;
import com.intellij.psi.impl.source.tree.FileElement;
import com.intellij.psi.tree.IFileElementType;
import java.util.Map;
import kotlin.Pair;
import com.intellij.core.CoreProjectEnvironment;
import com.intellij.ide.highlighter.JavaFileType;
import com.intellij.java.frontback.psi.impl.syntax.JavaSyntaxDefinitionExtension;
import com.intellij.lang.LanguageASTFactory;
import com.intellij.lang.java.JavaLanguage;
import com.intellij.lang.java.JavaParserDefinition;
import com.intellij.lang.java.syntax.JavaElementTypeConverterExtension;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.util.Disposer;
import com.intellij.openapi.util.registry.Registry;
import com.intellij.platform.syntax.psi.CommonElementTypeConverterFactory;
import com.intellij.platform.syntax.psi.ElementTypeConverters;
import com.intellij.platform.syntax.psi.LanguageSyntaxDefinitions;
import com.intellij.platform.syntax.psi.PsiSyntaxBuilderFactory;
import com.intellij.platform.syntax.psi.PsiSyntaxBuilderFactoryImpl;
import com.intellij.pom.java.LanguageLevel;
import com.intellij.psi.LanguageLevelKey;
import com.intellij.psi.PsiElement;
import com.intellij.psi.PsiErrorElement;
import com.intellij.psi.PsiFile;
import com.intellij.psi.PsiClass;
import com.intellij.psi.PsiModifier;
import com.intellij.psi.PsiModifierListOwner;
import com.intellij.psi.PsiFileFactory;
import com.intellij.psi.impl.DebugUtil;
import com.intellij.psi.impl.PsiFileFactoryImpl;
import com.intellij.psi.impl.source.tree.JavaASTFactory;
import com.intellij.testFramework.LightVirtualFile;

public class JavaPsiOracle {
  record Entry(String path, LanguageLevel level, String entry, Boolean trim) {}

  // The entries of the parser-partial tests (community/java/java-tests/testSrc/com/intellij/java/parser/partial).
  static final Map<String, JavaParserUtil.ParserWrapper> ENTRIES = Map.ofEntries(
    Map.entry("annotations", (b, l) -> new JavaParser(l).getDeclarationParser().parseAnnotations(b)),
    Map.entry("class_body", (b, l) -> new JavaParser(l).getDeclarationParser().parseClassBodyWithBraces(b, false, false)),
    Map.entry("class_body_annotation", (b, l) -> new JavaParser(l).getDeclarationParser().parseClassBodyWithBraces(b, true, false)),
    Map.entry("class_body_enum", (b, l) -> new JavaParser(l).getDeclarationParser().parseClassBodyWithBraces(b, false, true)),
    Map.entry("expression", (b, l) -> new JavaParser(l).getExpressionParser().parse(b)),
    Map.entry("file_parser", (b, l) -> new JavaParser(l).getFileParser().parse(b)),
    Map.entry("pattern", (b, l) -> {
      PatternParser parser = new JavaParser(l).getPatternParser();
      if (!parser.isPattern(b)) throw new IllegalArgumentException("Pattern is not expected");
      parser.parsePattern(b);
    }),
    Map.entry("reference", (b, l) -> new JavaParser(l).getReferenceParser().parseJavaCodeReference(b, false, false, false, false)),
    Map.entry("reference_incomplete", (b, l) -> new JavaParser(l).getReferenceParser().parseJavaCodeReference(b, true, false, false, false)),
    Map.entry("type", (b, l) -> new JavaParser(l).getReferenceParser().parseType(
      b, ReferenceParser.ELLIPSIS | ReferenceParser.DIAMONDS | ReferenceParser.DISJUNCTIONS)),
    Map.entry("type_params", (b, l) -> new JavaParser(l).getReferenceParser().parseTypeParameters(b)),
    Map.entry("code_block_deep", (b, l) -> new JavaParser(l).getStatementParser().parseCodeBlockDeep(b, true)),
    Map.entry("statements", (b, l) -> new JavaParser(l).getStatementParser().parseStatements(b)));

  // JavaParsingTestConfiguratorBase.kt: the test file root and the parse it runs.
  static final SyntaxElementType TEST_SYNTAX_TYPE = com.intellij.platform.syntax.SyntaxElementTypeKt.SyntaxElementType("test.java.file");
  static JavaParserUtil.ParserWrapper testParser;
  static LanguageLevel testLevel;
  static final IFileElementType TEST_FILE_TYPE = new IFileElementType("test.java.file", JavaLanguage.INSTANCE) {
    @Override
    public ASTNode parseContents(ASTNode chameleon) {
      PsiSyntaxBuilder psiBuilder = JavaParserUtil.createSyntaxBuilder(chameleon).getBuilder();
      psiBuilder.setDebugMode(true);
      SyntaxTreeBuilder builder = psiBuilder.getSyntaxTreeBuilder();
      SyntaxTreeBuilder.Marker root = builder.mark();
      testParser.parse(builder, testLevel);
      if (!builder.eof()) {
        SyntaxTreeBuilder.Marker unparsed = builder.mark();
        while (!builder.eof()) builder.advanceLexer();
        unparsed.error("Unparsed tokens");
      }
      root.done(TEST_SYNTAX_TYPE);
      return psiBuilder.getTreeBuilt().getFirstChildNode();
    }
  };
  static final ElementTypeConverter TEST_CONVERTER =
    ElementTypeConverterKt.elementTypeConverterOf(new Pair<>(TEST_SYNTAX_TYPE, TEST_FILE_TYPE));

  static final class TestPsiJavaFile extends PsiJavaFileImpl {
    TestPsiJavaFile(FileViewProvider viewProvider) { super(viewProvider); }

    @Override
    protected FileElement createFileElement(CharSequence text) {
      return new FileElement(TEST_FILE_TYPE, text);
    }
  }

  public static void main(String[] args) throws Exception {
    Path list = null;
    Path root = Path.of(".");
    Path out = null;
    LanguageLevel level = LanguageLevel.HIGHEST;
    boolean trim = false;
    boolean node = false;
    boolean getters = false;
    boolean serve = false;
    for (int i = 0; i < args.length; i++) {
      switch (args[i]) {
        case "--serve" -> serve = true;
        case "--list" -> list = Path.of(args[++i]);
        case "--root" -> root = Path.of(args[++i]);
        case "--out" -> out = Path.of(args[++i]);
        case "--level" -> level = LanguageLevel.valueOf(args[++i]);
        case "--trim" -> trim = true;
        case "--node" -> node = true;
        case "--getters" -> getters = true;
        default -> throw new IllegalArgumentException("unknown argument " + args[i]);
      }
    }
    if (!serve && (list == null || out == null)) {
      System.err.println("usage: JavaPsiOracle (--list FILE --out DIR [--root DIR] | --serve) [--level LEVEL] [--trim] [--node]");
      System.exit(2);
    }
    Path listFile = list;
    Path rootDir = root;
    Path outDir = out;
    LanguageLevel defaultLevel = level;
    Options options = new Options(trim, node, getters);
    if (getters) {
      // LOG.error logs in the IDE and goes on; DefaultLogger would throw.
      com.intellij.openapi.diagnostic.Logger.setFactory(category -> new com.intellij.openapi.diagnostic.DefaultLogger(category) {
        @Override
        public void error(String message, Throwable t, String... details) {}
      });
    }
    boolean serveMode = serve;
    Throwable[] failure = new Throwable[1];
    // PSI printing and the parser recurse on nesting depth; give them a deep stack.
    Thread worker = new Thread(null, () -> {
      try {
        Disposable disposable = Disposer.newDisposable();
        try {
          Project factory = createProject(disposable);
          if (!serveMode) {
            run(factory, readList(listFile, defaultLevel), rootDir, outDir, options);
            return;
          }
          BufferedReader input = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
          for (String request; (request = input.readLine()) != null; ) {
            if (request.isBlank()) continue;
            String[] parts = request.split("\t");
            run(factory, readList(Path.of(parts[0]), defaultLevel), Path.of(parts[1]), Path.of(parts[2]), options);
            System.out.println("done");
            System.out.flush();
          }
        } finally {
          Disposer.dispose(disposable);
        }
      } catch (Throwable t) {
        failure[0] = t;
      }
    }, "java-psi-oracle", 1L << 30);
    worker.start();
    worker.join();
    if (failure[0] != null) {
      failure[0].printStackTrace();
      System.exit(1);
    }
    System.exit(0);
  }

  record Options(boolean trim, boolean node, boolean getters) {}

  private static List<Entry> readList(Path list, LanguageLevel defaultLevel) throws IOException {
    List<Entry> entries = new ArrayList<>();
    for (String line : Files.readAllLines(list, StandardCharsets.UTF_8)) {
      if (line.isBlank()) continue;
      String[] fields = line.split("\t");
      LanguageLevel level = fields.length > 1 && !fields[1].isEmpty() ? LanguageLevel.valueOf(fields[1]) : defaultLevel;
      String entry = fields.length > 2 && !fields[2].isEmpty() ? fields[2] : "file";
      if (!entry.equals("file") && !ENTRIES.containsKey(entry)) throw new IllegalArgumentException("unknown entry " + entry);
      Boolean trim = fields.length > 3 && !fields[3].isEmpty() ? Boolean.valueOf(fields[3].equals("1")) : null;
      entries.add(new Entry(fields[0], level, entry, trim));
    }
    return entries;
  }

  // The parsing part of com.intellij.core.JavaCoreApplicationEnvironment (java-indexing-serializers,
  // not shipped in the IDE) at the oracle's revision.
  private static Project createProject(Disposable disposable) throws IOException {
    // PathManager looks for the installation around the jars; the pinned copy has none. Nothing is written there.
    if (System.getProperty("idea.home.path") == null) {
      Path home = Path.of(System.getProperty("java.io.tmpdir"), "psi-syntax-java-oracle-home");
      Files.createDirectories(home);
      System.setProperty("idea.home.path", home.toString());
    }
    CoreApplicationEnvironment app = new CoreApplicationEnvironment(disposable, true, true);
    app.registerFileType(JavaFileType.INSTANCE, "java");
    app.addExplicitExtension(LanguageASTFactory.INSTANCE, JavaLanguage.INSTANCE, new JavaASTFactory());
    app.addExplicitExtension(LanguageSyntaxDefinitions.getINSTANCE(), JavaLanguage.INSTANCE, new JavaSyntaxDefinitionExtension());
    app.addExplicitExtension(ElementTypeConverters.getInstance(), JavaLanguage.INSTANCE, new CommonElementTypeConverterFactory());
    app.addExplicitExtension(ElementTypeConverters.getInstance(), JavaLanguage.INSTANCE, new JavaElementTypeConverterExtension());
    app.registerParserDefinition(new JavaParserDefinition());
    app.registerApplicationService(PsiSyntaxBuilderFactory.class, new PsiSyntaxBuilderFactoryImpl());
    // As JavaCoreApplicationEnvironment does: no provider, so the augment steps of the getters are the identity.
    CoreApplicationEnvironment.registerApplicationExtensionPoint(
      com.intellij.psi.augment.PsiAugmentProvider.EP_NAME, com.intellij.psi.augment.PsiAugmentProvider.class);
    // JavaTestElementTypeConverterExtension of the partial tests: test.java.file to its IFileElementType.
    app.addExplicitExtension(ElementTypeConverters.getInstance(), JavaLanguage.INSTANCE, new ElementTypeConverterFactory() {
      @Override
      public ElementTypeConverter getElementTypeConverter() { return TEST_CONVERTER; }
    });
    CoreProjectEnvironment project = new CoreProjectEnvironment(disposable, app);
    return project.getProject();
  }

  private static String decode(byte[] bytes) throws IOException {
    return StandardCharsets.UTF_8.newDecoder()
      .onMalformedInput(CodingErrorAction.REPORT)
      .onUnmappableCharacter(CodingErrorAction.REPORT)
      .decode(ByteBuffer.wrap(bytes)).toString();
  }

  private static String trimText(String text) {
    int start = 0;
    int end = text.length();
    while (start < end && text.charAt(start) <= ' ') start++;
    while (end > start && text.charAt(end - 1) <= ' ') end--;
    return text.substring(start, end);
  }

  private static void run(Project project, List<Entry> entries, Path root, Path out, Options options) throws IOException {
    Files.createDirectories(out);
    try (BufferedWriter index = Files.newBufferedWriter(out.resolve("index.jsonl"), StandardCharsets.UTF_8)) {
      for (Entry entry : entries) {
        Path source = root.resolve(entry.path());
        String relative = root.toAbsolutePath().normalize().relativize(source.toAbsolutePath().normalize()).toString();
        String dumpName = relative + (options.getters() ? ".getters.txt" : ".psi.txt");
        StringBuilder line = new StringBuilder("{\"path\":").append(json(relative))
          .append(",\"dump\":").append(json(dumpName))
          .append(",\"level\":").append(json(entry.level().name()))
          .append(",\"entry\":").append(json(entry.entry()));
        try {
          String text = decode(Files.readAllBytes(source));
          // StringUtil.convertLineSeparators, as LoadTextUtil and ParsingTestFixture.loadFile do.
          boolean crlf = text.indexOf('\r') >= 0;
          if (crlf) text = text.replace("\r\n", "\n").replace('\r', '\n');
          if (entry.trim() != null ? entry.trim() : options.trim()) text = trimText(text);
          String name = source.getFileName().toString();
          PsiFile file;
          if (entry.entry().equals("file")) {
            // ParsingTestFixture.createFile
            LightVirtualFile virtualFile = new LightVirtualFile(name, JavaLanguage.INSTANCE, text);
            virtualFile.setCharset(StandardCharsets.UTF_8);
            file = ((PsiFileFactoryImpl) PsiFileFactory.getInstance(project)).trySetupPsiForFile(virtualFile, JavaLanguage.INSTANCE, false, false);
            if (file == null) throw new IllegalStateException("no PSI file for " + relative);
          } else {
            // JavaParsingTestConfiguratorBase.createPsiFile
            testParser = ENTRIES.get(entry.entry());
            testLevel = entry.level();
            LightVirtualFile virtualFile = new LightVirtualFile(name, JavaFileType.INSTANCE, text, -1);
            FileViewProvider viewProvider = new SingleRootFileViewProvider(PsiManager.getInstance(project), virtualFile, true);
            file = new TestPsiJavaFile(viewProvider);
          }
          file.putUserData(LanguageLevelKey.FILE_LANGUAGE_LEVEL_KEY, entry.level());
          String dump = options.getters() ? Getters.of(file) : trimText(DebugUtil.psiToString(file, true, false));
          Path dumpPath = out.resolve(dumpName);
          Files.createDirectories(dumpPath.getParent());
          Files.writeString(dumpPath, dump, StandardCharsets.UTF_8);
          if (options.node() && !options.getters()) {
            String nodeDump = trimText(DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true));
            Files.writeString(out.resolve(relative + ".node.txt"), nodeDump, StandardCharsets.UTF_8);
          }

          int errors = 0;
          PsiErrorElement first = null;
          ArrayDeque<PsiElement> stack = new ArrayDeque<>();
          stack.push(file);
          while (!stack.isEmpty()) {
            PsiElement element = stack.pop();
            if (element instanceof PsiErrorElement error) {
              errors++;
              if (first == null || error.getTextOffset() < first.getTextOffset()) first = error;
            }
            for (PsiElement child = element.getLastChild(); child != null; child = child.getPrevSibling()) {
              stack.push(child);
            }
          }
          line.append(",\"psiErrors\":").append(errors).append(",\"crlf\":").append(crlf).append(",\"firstError\":");
          if (first == null) {
            line.append("null");
          } else {
            int offset = first.getTextOffset();
            int lineNo = 1;
            int column = 1;
            for (int i = 0; i < offset && i < text.length(); i++) {
              if (text.charAt(i) == '\n') { lineNo++; column = 1; } else { column++; }
            }
            line.append("{\"offset\":").append(offset).append(",\"line\":").append(lineNo)
              .append(",\"column\":").append(column)
              .append(",\"message\":").append(json(first.getErrorDescription())).append('}');
          }
        } catch (Throwable t) {
          line.append(",\"failure\":").append(json(t.toString()));
        }
        index.write(line.append('}').toString());
        index.newLine();
      }
    }
  }

  // --getters: the PSI getters java_syntax::psi ports, by element type, in the order of
  // crates/java-syntax/src/psi/getters.rs.
  static final class Getters {
    // PsiModifier's constants, in their order (java_syntax::psi::modifier::Modifier::ALL).
    static final String[] MODIFIERS = {
      PsiModifier.PUBLIC, PsiModifier.PROTECTED, PsiModifier.PRIVATE, PsiModifier.PACKAGE_LOCAL, PsiModifier.STATIC,
      PsiModifier.ABSTRACT, PsiModifier.FINAL, PsiModifier.NATIVE, PsiModifier.SYNCHRONIZED, PsiModifier.STRICTFP,
      PsiModifier.TRANSIENT, PsiModifier.VOLATILE, PsiModifier.DEFAULT, PsiModifier.OPEN, PsiModifier.TRANSITIVE,
      PsiModifier.SEALED, PsiModifier.NON_SEALED, PsiModifier.VALUE};

    interface Answer { String get() throws Throwable; }

    final Map<ASTNode, Integer> ids = new java.util.IdentityHashMap<>();
    final StringBuilder out = new StringBuilder();
    final PsiFile file;
    int id;

    Getters(PsiFile file) { this.file = file; }

    static String of(PsiFile file) {
      Getters getters = new Getters(file);
      List<ASTNode> nodes = new ArrayList<>();
      ArrayDeque<ASTNode> stack = new ArrayDeque<>();
      stack.push(file.getNode());
      while (!stack.isEmpty()) {
        ASTNode node = stack.pop();
        getters.ids.put(node, nodes.size());
        nodes.add(node);
        for (ASTNode child = node.getLastChildNode(); child != null; child = child.getTreePrev()) stack.push(child);
      }
      for (int i = 0; i < nodes.size(); i++) {
        getters.id = i;
        getters.node(nodes.get(i));
      }
      return getters.out.toString();
    }

    void line(String getter, Answer answer) {
      String text;
      try {
        text = answer.get();
      }
      catch (Throwable t) {
        text = "!";
      }
      out.append(id).append(' ').append(getter).append(' ').append(text).append('\n');
    }

    String id(ASTNode node) {
      Integer id = node == null ? null : ids.get(node);
      return id == null ? "-" : id.toString();
    }

    String id(PsiElement element) {
      if (element == null) return "-";
      ASTNode node = element.getNode();
      Integer id = node == null ? null : ids.get(node);
      return id == null ? "-" : id.toString();
    }

    String list(PsiElement[] elements) {
      if (elements.length == 0) return ".";
      StringBuilder text = new StringBuilder();
      for (PsiElement element : elements) {
        if (text.length() > 0) text.append(',');
        text.append(id(element));
      }
      return text.toString();
    }

    interface Has { boolean has(String name) throws Throwable; }

    static String modifiers(Has has) throws Throwable {
      StringBuilder text = new StringBuilder();
      for (String name : MODIFIERS) {
        if (!has.has(name)) continue;
        if (text.length() > 0) text.append(',');
        text.append(name);
      }
      return text.length() == 0 ? "." : text.toString();
    }

    static String escape(String text) {
      return text.replace("\\", "\\\\").replace("\n", "\\n");
    }

    void owner(PsiModifierListOwner owner) {
      line("modifierList", () -> id(owner.getModifierList()));
      line("mods", () -> modifiers(owner::hasModifierProperty));
    }

    void node(ASTNode node) {
      // A token may share its type's name with a node type (the `module` keyword's is MODULE).
      if (!(node instanceof com.intellij.psi.impl.source.tree.CompositeElement)) return;
      PsiElement psi = node.getPsi();
      switch (node.getElementType().toString()) {
        case "CLASS", "ANONYMOUS_CLASS", "ENUM_CONSTANT_INITIALIZER", "IMPLICIT_CLASS" -> {
          PsiClass c = (PsiClass) psi;
          line("isInterface", () -> String.valueOf(c.isInterface()));
          line("isEnum", () -> String.valueOf(c.isEnum()));
          line("isRecord", () -> String.valueOf(c.isRecord()));
          line("isAnnotationType", () -> String.valueOf(c.isAnnotationType()));
          line("nameIdentifier", () -> id(c.getNameIdentifier()));
          line("extendsList", () -> id(c.getExtendsList()));
          line("implementsList", () -> id(c.getImplementsList()));
          line("permitsList", () -> id(c.getPermitsList()));
          line("recordHeader", () -> id(c.getRecordHeader()));
          line("typeParameterList", () -> id(c.getTypeParameterList()));
          line("containingClass", () -> id(c.getContainingClass()));
          owner(c);
          line("isValueClass", () -> String.valueOf(c.isValueClass()));
          line("keyword", () -> node instanceof com.intellij.psi.impl.source.tree.java.ClassElement e
            ? id(e.findChildByRole(com.intellij.psi.impl.source.tree.ChildRole.CLASS_OR_INTERFACE_KEYWORD)) : "-");
          if (psi instanceof com.intellij.psi.impl.source.PsiExtensibleClass e) {
            line("ownFields", () -> list(e.getOwnFields().toArray(PsiElement.EMPTY_ARRAY)));
            line("ownMethods", () -> list(e.getOwnMethods().toArray(PsiElement.EMPTY_ARRAY)));
            line("ownInnerClasses", () -> list(e.getOwnInnerClasses().toArray(PsiElement.EMPTY_ARRAY)));
          }
          if (psi instanceof com.intellij.psi.PsiAnonymousClass a && node.getElementType().toString().equals("ANONYMOUS_CLASS")) {
            line("baseClassReference", () -> id(a.getBaseClassReference()));
            line("argumentList", () -> id(a.getArgumentList()));
          }
        }
        case "EXTENDS_LIST", "IMPLEMENTS_LIST", "PERMITS_LIST", "THROWS_LIST", "EXTENDS_BOUND_LIST", "PROVIDES_WITH_LIST" ->
          line("refs", () -> list(((com.intellij.psi.PsiReferenceList) psi).getReferenceElements()));
        case "METHOD", "ANNOTATION_METHOD" -> {
          com.intellij.psi.PsiMethod m = (com.intellij.psi.PsiMethod) psi;
          line("isConstructor", () -> String.valueOf(m.isConstructor()));
          line("returnTypeElement", () -> id(m.getReturnTypeElement()));
          line("parameterList", () -> id(m.getParameterList()));
          line("typeParameterList", () -> id(m.getTypeParameterList()));
          line("throwsList", () -> id(m.getThrowsList()));
          line("body", () -> id(m.getBody()));
          line("isVarArgs", () -> String.valueOf(m.isVarArgs()));
          line("isCompactConstructor", () -> String.valueOf(com.intellij.psi.util.JavaPsiRecordUtil.isCompactConstructor(m)));
          if (!m.isConstructor()) line("returnDims", () -> String.valueOf(m.getReturnType().getArrayDimensions()));
          line("containingClass", () -> id(m.getContainingClass()));
          line("nameIdentifier", () -> id(m.getNameIdentifier()));
          owner(m);
        }
        case "FIELD" -> {
          com.intellij.psi.PsiField f = (com.intellij.psi.PsiField) psi;
          line("typeElement", () -> id(f.getTypeElement()));
          line("nameIdentifier", () -> id(f.getNameIdentifier()));
          line("initializer", () -> id(f.getInitializer()));
          if (f.getTypeElement() != null) line("dims", () -> String.valueOf(f.getType().getArrayDimensions()));
          line("containingClass", () -> id(f.getContainingClass()));
          owner(f);
        }
        case "ENUM_CONSTANT" -> {
          com.intellij.psi.PsiEnumConstant e = (com.intellij.psi.PsiEnumConstant) psi;
          line("typeElement", () -> id(e.getTypeElement()));
          line("nameIdentifier", () -> id(e.getNameIdentifier()));
          line("initializer", () -> id(e.getInitializer()));
          line("argumentList", () -> id(e.getArgumentList()));
          line("initializingClass", () -> id(e.getInitializingClass()));
          line("containingClass", () -> id(e.getContainingClass()));
          owner(e);
        }
        case "LOCAL_VARIABLE", "RESOURCE_VARIABLE" -> {
          com.intellij.psi.PsiLocalVariable v = (com.intellij.psi.PsiLocalVariable) psi;
          line("typeElement", () -> id(v.getTypeElement()));
          line("nameIdentifier", () -> id(v.getNameIdentifier()));
          line("initializer", () -> id(v.getInitializer()));
          if (typed(v::getTypeElement)) line("dims", () -> String.valueOf(v.getType().getArrayDimensions()));
          owner(v);
        }
        case "PARAMETER" -> {
          com.intellij.psi.PsiParameter p = (com.intellij.psi.PsiParameter) psi;
          line("typeElement", () -> id(p.getTypeElement()));
          line("nameIdentifier", () -> id(p.getNameIdentifier()));
          if (typed(p::getTypeElement)) line("dims", () -> String.valueOf(p.getType().getArrayDimensions()));
          line("isVarArgs", () -> String.valueOf(p.isVarArgs()));
          owner(p);
        }
        case "RECORD_COMPONENT" -> {
          com.intellij.psi.PsiRecordComponent r = (com.intellij.psi.PsiRecordComponent) psi;
          line("typeElement", () -> id(r.getTypeElement()));
          line("nameIdentifier", () -> id(r.getNameIdentifier()));
          line("dims", () -> String.valueOf(r.getType().getArrayDimensions()));
          line("isVarArgs", () -> String.valueOf(r.isVarArgs()));
          line("containingClass", () -> id(r.getContainingClass()));
          owner(r);
        }
        case "PATTERN_VARIABLE" -> {
          com.intellij.psi.PsiPatternVariable v = (com.intellij.psi.PsiPatternVariable) psi;
          line("typeElement", () -> id(v.getTypeElement()));
          line("nameIdentifier", () -> id(v.getNameIdentifier()));
          owner(v);
        }
        case "RECEIVER" -> {
          com.intellij.psi.PsiReceiverParameter r = (com.intellij.psi.PsiReceiverParameter) psi;
          line("typeElement", () -> id(r.getTypeElement()));
          owner(r);
        }
        case "DECONSTRUCTION_PATTERN_VARIABLE", "TYPE_PARAMETER" -> {
          line("nameIdentifier", () -> id(((com.intellij.psi.PsiNameIdentifierOwner) psi).getNameIdentifier()));
          owner((PsiModifierListOwner) psi);
          if (psi instanceof com.intellij.psi.PsiVariable v) line("typeElement", () -> id(v.getTypeElement()));
        }
        case "CLASS_INITIALIZER" -> {
          com.intellij.psi.PsiClassInitializer i = (com.intellij.psi.PsiClassInitializer) psi;
          line("containingClass", () -> id(i.getContainingClass()));
          owner(i);
        }
        case "MODULE" -> {
          owner((PsiModifierListOwner) psi);
          line("nameIdentifier", () -> id(((com.intellij.psi.PsiNameIdentifierOwner) psi).getNameIdentifier()));
        }
        case "REQUIRES_STATEMENT" -> owner((PsiModifierListOwner) psi);
        case "LABELED_STATEMENT" -> line("nameIdentifier", () -> id(((com.intellij.psi.PsiNameIdentifierOwner) psi).getNameIdentifier()));
        case "MODIFIER_LIST" -> {
          com.intellij.psi.PsiModifierList l = (com.intellij.psi.PsiModifierList) psi;
          line("explicit", () -> modifiers(l::hasExplicitModifier));
          line("modifiers", () -> modifiers(l::hasModifierProperty));
          line("annotations", () -> list(l.getAnnotations()));
        }
        case "TYPE" -> {
          com.intellij.psi.PsiTypeElement t = (com.intellij.psi.PsiTypeElement) psi;
          line("innermost", () -> id(t.getInnermostComponentReferenceElement()));
          line("inferred", () -> String.valueOf(t.isInferredType()));
          boolean unnamedPattern = t.getFirstChild() == null && t.getParent() instanceof com.intellij.psi.PsiUnnamedPattern;
          if (!t.isInferredType() && !unnamedPattern) {
            line("type", () -> shape(t.getType()));
            line("component", () -> component(t.getType().getDeepComponentType()));
          }
        }
        case "DIAMOND_TYPE" -> line("type", () -> shape(((com.intellij.psi.PsiTypeElement) psi).getType()));
        case "JAVA_CODE_REFERENCE" -> {
          com.intellij.psi.impl.source.PsiJavaCodeReferenceElementImpl r = (com.intellij.psi.impl.source.PsiJavaCodeReferenceElementImpl) psi;
          line("name", () -> id(r.getReferenceNameElement()));
          line("qualifier", () -> id(r.getQualifier()));
          line("params", () -> id(r.getParameterList()));
          line("qualified", () -> String.valueOf(r.isQualified()));
          line("kind", () -> r.getKindEnum(file).name());
        }
        case "IMPORT_STATIC_REFERENCE" -> {
          com.intellij.psi.PsiImportStaticReferenceElement r = (com.intellij.psi.PsiImportStaticReferenceElement) psi;
          line("name", () -> id(r.getReferenceNameElement()));
          line("classReference", () -> id(r.getClassReference()));
        }
        case "REFERENCE_EXPRESSION" -> {
          com.intellij.psi.PsiReferenceExpression r = (com.intellij.psi.PsiReferenceExpression) psi;
          line("qualifier", () -> id(r.getQualifierExpression()));
          line("name", () -> id(r.getReferenceNameElement()));
          line("params", () -> id(r.getParameterList()));
        }
        case "METHOD_REF_EXPRESSION" -> {
          com.intellij.psi.PsiMethodReferenceExpression r = (com.intellij.psi.PsiMethodReferenceExpression) psi;
          line("qualifier", () -> id(r.getQualifier()));
          line("name", () -> id(r.getReferenceNameElement()));
          line("isConstructor", () -> String.valueOf(r.isConstructor()));
        }
        case "THIS_EXPRESSION" -> line("qualifier", () -> id(((com.intellij.psi.PsiThisExpression) psi).getQualifier()));
        case "SUPER_EXPRESSION" -> line("qualifier", () -> id(((com.intellij.psi.PsiSuperExpression) psi).getQualifier()));
        case "METHOD_CALL_EXPRESSION" -> {
          com.intellij.psi.PsiMethodCallExpression c = (com.intellij.psi.PsiMethodCallExpression) psi;
          line("methodExpression", () -> id(c.getMethodExpression()));
          line("argumentList", () -> id(c.getArgumentList()));
        }
        case "EXPRESSION_LIST" -> {
          com.intellij.psi.PsiExpressionList l = (com.intellij.psi.PsiExpressionList) psi;
          line("expressions", () -> list(l.getExpressions()));
          line("count", () -> String.valueOf(l.getExpressionCount()));
        }
        case "NEW_EXPRESSION" -> {
          com.intellij.psi.PsiNewExpression n = (com.intellij.psi.PsiNewExpression) psi;
          line("qualifier", () -> id(n.getQualifier()));
          line("classReference", () -> id(n.getClassReference()));
          line("classOrAnonymous", () -> id(n.getClassOrAnonymousClassReference()));
          line("anonymousClass", () -> id(n.getAnonymousClass()));
          line("argumentList", () -> id(n.getArgumentList()));
          line("typeArgumentList", () -> id(n.getTypeArgumentList()));
          line("arrayDimensions", () -> list(n.getArrayDimensions()));
          line("arrayInitializer", () -> id(n.getArrayInitializer()));
          line("isArrayCreation", () -> String.valueOf(n.isArrayCreation()));
        }
        case "IMPORT_STATEMENT", "IMPORT_STATIC_STATEMENT" -> {
          com.intellij.psi.PsiImportStatementBase i = (com.intellij.psi.PsiImportStatementBase) psi;
          line("isOnDemand", () -> String.valueOf(i.isOnDemand()));
          line("importReference", () -> id(i.getImportReference()));
        }
        case "IMPORT_MODULE_STATEMENT" -> {
          com.intellij.psi.PsiImportModuleStatement i = (com.intellij.psi.PsiImportModuleStatement) psi;
          line("isOnDemand", () -> String.valueOf(i.isOnDemand()));
          line("moduleReference", () -> id(i.getModuleReference()));
        }
        case "PACKAGE_STATEMENT" -> {
          com.intellij.psi.PsiPackageStatement p = (com.intellij.psi.PsiPackageStatement) psi;
          line("packageReference", () -> id(p.getPackageReference()));
          line("packageName", () -> escape(p.getPackageName()));
        }
        case "ANNOTATION" -> {
          com.intellij.psi.PsiAnnotation a = (com.intellij.psi.PsiAnnotation) psi;
          line("nameReference", () -> id(a.getNameReferenceElement()));
          line("referenceText", () -> {
            com.intellij.psi.PsiJavaCodeReferenceElement name = a.getNameReferenceElement();
            return name == null ? "-" : escape(com.intellij.psi.impl.source.tree.JavaSourceUtil.getReferenceText(name));
          });
          line("parameterList", () -> id(a.getParameterList()));
        }
        case "TYPE_CAST_EXPRESSION" -> {
          com.intellij.psi.PsiTypeCastExpression c = (com.intellij.psi.PsiTypeCastExpression) psi;
          line("castType", () -> id(c.getCastType()));
          line("operand", () -> id(c.getOperand()));
        }
        case "INSTANCE_OF_EXPRESSION" -> {
          com.intellij.psi.PsiInstanceOfExpression i = (com.intellij.psi.PsiInstanceOfExpression) psi;
          line("operand", () -> id(i.getOperand()));
          line("checkType", () -> id(i.getCheckType()));
          line("pattern", () -> id(i.getPattern()));
        }
        case "LAMBDA_EXPRESSION" -> {
          com.intellij.psi.PsiLambdaExpression l = (com.intellij.psi.PsiLambdaExpression) psi;
          line("parameterList", () -> id(l.getParameterList()));
          line("body", () -> id(l.getBody()));
        }
        case "PARAMETER_LIST" -> line("parameters", () -> list(((com.intellij.psi.PsiParameterList) psi).getParameters()));
        case "RECORD_HEADER" -> line("components", () -> list(((com.intellij.psi.PsiRecordHeader) psi).getRecordComponents()));
        case "TYPE_PARAMETER_LIST" -> line("typeParameters", () -> list(((com.intellij.psi.PsiTypeParameterList) psi).getTypeParameters()));
        default -> {}
      }
    }

    // A variable with a type element that is not `var` (the dimensions of the others resolve).
    interface TypeElementGetter { com.intellij.psi.PsiTypeElement get(); }

    static boolean typed(TypeElementGetter getter) {
      try {
        com.intellij.psi.PsiTypeElement t = getter.get();
        return t != null && !t.isInferredType();
      }
      catch (Throwable e) {
        return false;
      }
    }

    // The class of the outermost PsiType and its getArrayDimensions().
    static String shape(com.intellij.psi.PsiType t) {
      String kind;
      if (t instanceof com.intellij.psi.PsiEllipsisType) kind = "ellipsis";
      else if (t instanceof com.intellij.psi.PsiArrayType) kind = "array";
      else if (t == com.intellij.psi.PsiTypes.nullType()) kind = "null";
      else if (t instanceof com.intellij.psi.PsiPrimitiveType) kind = "primitive";
      else if (t instanceof com.intellij.psi.PsiClassType) kind = "class";
      else if (t instanceof com.intellij.psi.PsiWildcardType) kind = "wildcard";
      else if (t instanceof com.intellij.psi.PsiIntersectionType) kind = "intersection";
      else if (t instanceof com.intellij.psi.PsiDisjunctionType) kind = "disjunction";
      else if (t instanceof com.intellij.psi.PsiDiamondType) kind = "diamond";
      else kind = t.getClass().getSimpleName();
      return kind + " " + t.getArrayDimensions();
    }

    // The deep component type (getDeepComponentType()): its class as `shape` names it, with the
    // reference element of a class type and the name of a primitive.
    String component(com.intellij.psi.PsiType t) {
      String kind = shape(t);
      kind = kind.substring(0, kind.indexOf(' '));
      if (t instanceof com.intellij.psi.impl.source.PsiClassReferenceType r) return kind + " " + id(r.getReference());
      if (t instanceof com.intellij.psi.PsiPrimitiveType p && t != com.intellij.psi.PsiTypes.nullType()) return kind + " " + p.getName();
      return kind;
    }
  }

  private static String json(String value) {
    StringBuilder builder = new StringBuilder("\"");
    for (int i = 0; i < value.length(); i++) {
      char c = value.charAt(i);
      switch (c) {
        case '"' -> builder.append("\\\"");
        case '\\' -> builder.append("\\\\");
        case '\n' -> builder.append("\\n");
        case '\r' -> builder.append("\\r");
        case '\t' -> builder.append("\\t");
        default -> {
          if (c < 0x20) builder.append(String.format("\\u%04x", (int) c));
          else builder.append(c);
        }
      }
    }
    return builder.append('"').toString();
  }
}
