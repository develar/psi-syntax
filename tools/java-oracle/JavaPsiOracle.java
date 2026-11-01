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
    Options options = new Options(trim, node);
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

  record Options(boolean trim, boolean node) {}

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
        String dumpName = relative + ".psi.txt";
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
          String dump = trimText(DebugUtil.psiToString(file, true, false));
          Path dumpPath = out.resolve(dumpName);
          Files.createDirectories(dumpPath.getParent());
          Files.writeString(dumpPath, dump, StandardCharsets.UTF_8);
          if (options.node()) {
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
