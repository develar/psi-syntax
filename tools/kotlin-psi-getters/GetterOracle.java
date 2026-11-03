// The Kotlin PSI getter oracle: what Kotlin 2.4.21's own PSI classes answer for every element of each listed file,
// one line per element and getter, in the format `kotlin_syntax::psi`'s printer (crates/kotlin-syntax/tests/common/
// getters.rs) writes for the same file, so `diff` compares the port with the PSI method by method.
//
//   java -Xss512m -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" tools/kotlin-psi-getters/GetterOracle.java \
//     --list LIST --root ROOT --out OUT
//
// writes OUT/<path>.getters.txt per listed file (a .kts name is parsed as a script); --hash OUT writes instead one
// line per file to the file OUT, `<path>\t<FNV-1a 64 of the lines' UTF-8, hex>\t<line count>`, for corpora whose
// lines would not fit on disk (the example `psi_getters --hash` prints the same); --serve reads
// "LIST<TAB>ROOT<TAB>OUT" lines from stdin and prints "done" after each, as PsiOracle.java does. --classes prints,
// for every element type met in the listed files, the PSI interfaces and classes its element is an instance of
// (the tables the printer dispatches on).
//
// A line is `<start>..<end> <ELEMENT_TYPE> <getter>=<answer>`: offsets are UTF-8 byte offsets in the text after
// kotlinc's line separator conversion (CRLF and CR to LF, as PsiOracle.java parses), an element answer is
// `ELEMENT_TYPE@start..end` or `null`, a list is `[a,b]`, a string is quoted with `\\`, `\"`, `\n`, `\r`, `\t`
// escaped. A getter that throws (IntelliJ's `!!`, a required child that is missing) prints `null`: the port answers
// None there; the count of such calls goes to stderr.

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.function.Supplier;

import org.jetbrains.kotlin.cli.CompilerConfigurationCreationKt;
import org.jetbrains.kotlin.cli.common.messages.MessageCollector;
import org.jetbrains.kotlin.cli.jvm.compiler.EnvironmentConfigFiles;
import org.jetbrains.kotlin.cli.jvm.compiler.KotlinCoreEnvironment;
import org.jetbrains.kotlin.com.intellij.lang.ASTNode;
import org.jetbrains.kotlin.com.intellij.openapi.Disposable;
import org.jetbrains.kotlin.com.intellij.openapi.util.Disposer;
import org.jetbrains.kotlin.com.intellij.psi.PsiElement;
import org.jetbrains.kotlin.com.intellij.psi.impl.source.tree.LeafElement;
import org.jetbrains.kotlin.config.CompilerConfiguration;
import org.jetbrains.kotlin.lexer.KtModifierKeywordToken;
import org.jetbrains.kotlin.lexer.KtTokens;
import org.jetbrains.kotlin.name.FqName;
import org.jetbrains.kotlin.name.Name;
import org.jetbrains.kotlin.psi.*;
import org.jetbrains.kotlin.psi.psiUtil.KtPsiUtilKt;

public class GetterOracle {
  static int thrown;

  public static void main(String[] args) throws Exception {
    Path list = null;
    Path root = Path.of(".");
    Path out = null;
    boolean serve = false;
    boolean classes = false;
    Path hash = null;
    for (int i = 0; i < args.length; i++) {
      switch (args[i]) {
        case "--serve" -> serve = true;
        case "--classes" -> classes = true;
        case "--list" -> list = Path.of(args[++i]);
        case "--root" -> root = Path.of(args[++i]);
        case "--out" -> out = Path.of(args[++i]);
        case "--hash" -> hash = Path.of(args[++i]);
        default -> throw new IllegalArgumentException("unknown argument " + args[i]);
      }
    }
    if (!serve && (list == null || (out == null && hash == null && !classes))) {
      System.err.println("usage: GetterOracle (--list FILE [--root DIR] (--out DIR | --hash FILE | --classes) | --serve)");
      System.exit(2);
    }
    Path listFile = list;
    Path rootDir = root;
    Path outDir = out;
    boolean serveMode = serve;
    boolean classesMode = classes;
    Path hashFile = hash;
    Throwable[] failure = new Throwable[1];
    Thread worker = new Thread(null, () -> {
      try {
        Disposable disposable = Disposer.newDisposable();
        try {
          KtPsiFactory factory = createFactory(disposable);
          if (classesMode) {
            classes(factory, readList(listFile), rootDir);
          } else if (hashFile != null) {
            hash(factory, readList(listFile), rootDir, hashFile);
          } else if (!serveMode) {
            run(factory, readList(listFile), rootDir, outDir);
          } else {
            BufferedReader input = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            for (String request; (request = input.readLine()) != null; ) {
              if (request.isBlank()) continue;
              String[] parts = request.split("\t");
              run(factory, readList(Path.of(parts[0])), Path.of(parts[1]), Path.of(parts[2]));
              System.out.println("done");
              System.out.flush();
            }
          }
        } finally {
          Disposer.dispose(disposable);
        }
      } catch (Throwable t) {
        failure[0] = t;
      }
    }, "getter-oracle", 1L << 30);
    worker.start();
    worker.join();
    if (failure[0] != null) {
      failure[0].printStackTrace();
      System.exit(1);
    }
    System.err.println("getters that threw: " + thrown);
    System.exit(0);
  }

  static List<String> readList(Path list) throws IOException {
    List<String> paths = new ArrayList<>();
    for (String line : Files.readAllLines(list, StandardCharsets.UTF_8)) {
      if (!line.isBlank()) paths.add(line.split("\t")[0]);
    }
    return paths;
  }

  static KtPsiFactory createFactory(Disposable disposable) {
    CompilerConfiguration configuration = CompilerConfigurationCreationKt.create(
      CompilerConfiguration.Companion, null, MessageCollector.Companion.getNONE());
    KotlinCoreEnvironment environment = KotlinCoreEnvironment.createForProduction(
      disposable, configuration, EnvironmentConfigFiles.JVM_CONFIG_FILES);
    return new KtPsiFactory(environment.getProject(), false);
  }

  static KtFile parse(KtPsiFactory factory, Path source) throws IOException {
    String text = Files.readString(source, StandardCharsets.UTF_8);
    text = text.replace("\r\n", "\n").replace('\r', '\n');
    return factory.createFile(source.getFileName().toString(), text);
  }

  static void run(KtPsiFactory factory, List<String> paths, Path root, Path out) throws IOException {
    for (String entry : paths) {
      Path source = root.resolve(entry);
      String relative = root.toAbsolutePath().normalize().relativize(source.toAbsolutePath().normalize()).toString();
      Path target = out.resolve(relative + ".getters.txt");
      Files.createDirectories(target.getParent());
      String lines;
      try {
        KtFile file = parse(factory, source);
        lines = new Printer(file.getText()).file(file);
      } catch (Throwable t) {
        lines = "failure " + t + "\n";
      }
      Files.writeString(target, lines, StandardCharsets.UTF_8);
    }
  }

  static void hash(KtPsiFactory factory, List<String> paths, Path root, Path out) throws IOException {
    try (var w = Files.newBufferedWriter(out, StandardCharsets.UTF_8)) {
      for (String entry : paths) {
        String lines;
        try {
          KtFile file = parse(factory, root.resolve(entry));
          lines = new Printer(file.getText()).file(file);
        } catch (Throwable t) {
          lines = "failure " + t + "\n";
        }
        long h = 0xcbf29ce484222325L;
        for (byte b : lines.getBytes(StandardCharsets.UTF_8)) {
          h ^= (b & 0xff);
          h *= 0x100000001b3L;
        }
        long count = lines.chars().filter(c -> c == '\n').count();
        w.write(entry + "\t" + String.format("%016x", h) + "\t" + count + "\n");
      }
    }
  }

  // The element types' PSI classes, for the tables of the printer.
  static final Class<?>[] CLASSES = {
    KtExpression.class, KtDeclaration.class, KtModifierListOwner.class, KtAnnotated.class, KtNamedDeclaration.class,
    KtCallableDeclaration.class, KtTypeParameterListOwner.class, KtClassOrObject.class, KtClass.class,
    KtCallElement.class, KtTypeElement.class, KtSimpleNameExpression.class, KtQualifiedExpression.class,
    KtValueArgument.class, KtSuperTypeListEntry.class, KtDeclarationWithBody.class, KtFunction.class,
  };

  static void classes(KtPsiFactory factory, List<String> paths, Path root) throws IOException {
    Map<String, TreeSet<String>> seen = new TreeMap<>();
    for (String entry : paths) {
      KtFile file = parse(factory, root.resolve(entry));
      ArrayDeque<PsiElement> stack = new ArrayDeque<>();
      stack.push(file);
      while (!stack.isEmpty()) {
        PsiElement e = stack.pop();
        for (PsiElement c = e.getLastChild(); c != null; c = c.getPrevSibling()) stack.push(c);
        if (e.getNode() instanceof LeafElement) continue;
        StringBuilder is = new StringBuilder(e.getClass().getSimpleName());
        for (Class<?> k : CLASSES) if (k.isInstance(e)) is.append(' ').append(k.getSimpleName());
        seen.computeIfAbsent(e.getNode().getElementType().toString(), k -> new TreeSet<>()).add(is.toString());
      }
    }
    for (var e : seen.entrySet()) for (String s : e.getValue()) System.out.println(e.getKey() + "\t" + s);
  }

  static final class Printer {
    final StringBuilder out = new StringBuilder();
    final int[] utf8; // utf8[i]: the UTF-8 offset of UTF-16 index i
    String prefix;

    Printer(String text) {
      utf8 = new int[text.length() + 1];
      int bytes = 0;
      for (int i = 0; i < text.length(); i++) {
        utf8[i] = bytes;
        char c = text.charAt(i);
        if (c < 0x80) bytes += 1;
        else if (c < 0x800) bytes += 2;
        else if (Character.isHighSurrogate(c) && i + 1 < text.length() && Character.isLowSurrogate(text.charAt(i + 1))) {
          bytes += 4;
          utf8[++i] = bytes;
          continue;
        } else bytes += 3;
      }
      utf8[text.length()] = bytes;
    }

    String range(PsiElement e) {
      var r = e.getTextRange();
      return utf8[r.getStartOffset()] + ".." + utf8[r.getEndOffset()];
    }

    String el(PsiElement e) {
      return e == null ? "null" : e.getNode().getElementType() + "@" + range(e);
    }

    String node(ASTNode n) {
      return n == null ? "null" : el(n.getPsi());
    }

    String els(List<? extends PsiElement> es) {
      StringBuilder b = new StringBuilder("[");
      for (PsiElement e : es) {
        if (b.length() > 1) b.append(',');
        b.append(el(e));
      }
      return b.append(']').toString();
    }

    static String str(Object s) {
      if (s == null) return "null";
      String v = s.toString();
      StringBuilder b = new StringBuilder("\"");
      for (int i = 0; i < v.length(); i++) {
        char c = v.charAt(i);
        switch (c) {
          case '\\' -> b.append("\\\\");
          case '"' -> b.append("\\\"");
          case '\n' -> b.append("\\n");
          case '\r' -> b.append("\\r");
          case '\t' -> b.append("\\t");
          default -> b.append(c);
        }
      }
      return b.append('"').toString();
    }

    void line(String getter, Supplier<String> answer) {
      String a;
      try {
        a = answer.get();
      } catch (RuntimeException | AssertionError t) {
        thrown++;
        a = "null";
      }
      out.append(prefix).append(getter).append('=').append(a).append('\n');
    }

    String file(KtFile file) {
      ArrayDeque<PsiElement> stack = new ArrayDeque<>();
      stack.push(file);
      while (!stack.isEmpty()) {
        PsiElement e = stack.pop();
        for (PsiElement c = e.getLastChild(); c != null; c = c.getPrevSibling()) stack.push(c);
        if (e.getNode() instanceof LeafElement) continue;
        prefix = range(e) + " " + e.getNode().getElementType() + " ";
        element(e);
      }
      return out.toString();
    }

    // The order of the checks is the printer's.
    void element(PsiElement e) {
      if (e instanceof KtExpression) line("expression", () -> "true");
      if (e instanceof KtFile f) line("fileAnnotationList", () -> el(f.getFileAnnotationList()));
      if (e instanceof KtModifierListOwner m) {
        line("modifierList", () -> el(m.getModifierList()));
        line("modifiers", () -> {
          StringBuilder b = new StringBuilder("[");
          for (KtModifierKeywordToken t : KtTokens.MODIFIER_KEYWORDS_ARRAY) {
            if (m.hasModifier(t)) {
              if (b.length() > 1) b.append(',');
              b.append(t.getValue());
            }
          }
          return b.append(']').toString();
        });
        line("visibilityModifier", () -> el(KtPsiUtilKt.visibilityModifier(m)));
        line("visibilityModifierType", () -> str(KtPsiUtilKt.visibilityModifierType(m)));
        line("isPrivate", () -> "" + KtPsiUtilKt.isPrivate(m));
        line("isProtected", () -> "" + KtPsiUtilKt.isProtected(m));
      }
      if (e instanceof KtDeclaration d) {
        line("modalityModifier", () -> el(KtPsiUtilKt.modalityModifier(d)));
        line("modalityModifierType", () -> str(KtPsiUtilKt.modalityModifierType(d)));
        line("containingClassOrObject", () -> el(KtPsiUtilKt.getContainingClassOrObject(d)));
        line("containingScript", () -> el(KtPsiUtilKt.getContainingScript(d)));
        line("containingClassOrScript", () -> el(KtPsiUtilKt.getContainingClassOrScript(d)));
      }
      if (e instanceof KtClassBody b) line("containingClassOrObject", () -> el(KtPsiUtilKt.getContainingClassOrObject(b)));
      if (e instanceof KtConstructor<?> c) line("containingClassOrObject", () -> el(c.getContainingClassOrObject()));
      if (e instanceof KtAnnotated a) line("annotationEntries", () -> els(a.getAnnotationEntries()));
      if (e instanceof KtModifierList ml) line("annotationEntries", () -> els(ml.getAnnotationEntries()));
      if (e instanceof KtAnnotation an) {
        line("entries", () -> els(an.getEntries()));
        line("useSiteTarget", () -> el(an.getUseSiteTarget()));
      }
      if (e instanceof KtAnnotationEntry en) {
        line("shortName", () -> str(en.getShortName()));
        line("useSiteTarget", () -> el(en.getUseSiteTarget()));
        line("typeReference", () -> el(en.getTypeReference()));
        line("atSymbol", () -> el(en.getAtSymbol()));
      }
      if (e instanceof KtAnnotationUseSiteTarget t) {
        line("annotationUseSiteTarget", () -> str(t.getAnnotationUseSiteTarget()));
      }
      if (e instanceof KtNamedDeclaration n && !(e instanceof KtScript)) {
        line("name", () -> str(n.getName()));
        line("nameIdentifier", () -> el(n.getNameIdentifier()));
      }
      if (e instanceof KtImportAlias ia) {
        line("name", () -> str(ia.getName()));
        line("nameIdentifier", () -> el(ia.getNameIdentifier()));
      }
      if (e instanceof KtClassOrObject co) {
        line("isTopLevel", () -> "" + co.isTopLevel());
        line("isData", () -> "" + co.isData());
        line("isAnnotation", () -> "" + co.isAnnotation());
        line("declarationKeyword", () -> el(co.getDeclarationKeyword()));
        line("superTypeList", () -> el(co.getSuperTypeList()));
        line("superTypeListEntries", () -> els(co.getSuperTypeListEntries()));
        line("body", () -> el(co.getBody()));
        line("primaryConstructor", () -> el(co.getPrimaryConstructor()));
        line("primaryConstructorParameterList", () -> el(co.getPrimaryConstructorParameterList()));
        line("primaryConstructorParameters", () -> els(co.getPrimaryConstructorParameters()));
        line("secondaryConstructors", () -> els(co.getSecondaryConstructors()));
      }
      if (e instanceof KtClass k) {
        line("isInterface", () -> "" + k.isInterface());
        line("isEnum", () -> "" + k.isEnum());
        line("isSealed", () -> "" + k.isSealed());
        line("isInner", () -> "" + k.isInner());
        line("isInline", () -> "" + k.isInline());
        line("isValue", () -> "" + k.isValue());
        line("funKeyword", () -> el(k.getFunKeyword()));
      }
      if (e instanceof KtObjectDeclaration o) {
        line("isCompanion", () -> "" + o.isCompanion());
        line("isObjectLiteral", () -> "" + o.isObjectLiteral());
        line("objectKeyword", () -> el(o.getObjectKeyword()));
      }
      if (e instanceof KtSuperTypeListEntry s) {
        line("typeReference", () -> el(s.getTypeReference()));
        line("typeAsUserType", () -> el(s.getTypeAsUserType()));
      }
      if (e instanceof KtCallableDeclaration cd) {
        line("receiverTypeReference", () -> el(cd.getReceiverTypeReference()));
        line("typeReference", () -> el(cd.getTypeReference()));
        line("valueParameterList", () -> el(cd.getValueParameterList()));
        line("valueParameters", () -> els(cd.getValueParameters()));
      }
      if (e instanceof KtTypeParameterListOwner tp) {
        line("typeParameterList", () -> el(tp.getTypeParameterList()));
        line("typeParameters", () -> els(tp.getTypeParameters()));
        line("typeConstraintList", () -> el(tp.getTypeConstraintList()));
        line("typeConstraints", () -> els(tp.getTypeConstraints()));
      }
      if (e instanceof KtNamedFunction || e instanceof KtProperty || e instanceof KtPropertyAccessor) {
        line("isExtensionDeclaration", () -> "" + KtPsiUtilKt.isExtensionDeclaration(e));
      }
      if (e instanceof KtNamedFunction f) {
        line("isLocal", () -> "" + f.isLocal());
        line("isAnonymous", () -> "" + f.isAnonymous());
        line("isTopLevel", () -> "" + f.isTopLevel());
        line("initializer", () -> el(f.getInitializer()));
      }
      if (e instanceof KtProperty p) {
        line("isLocal", () -> "" + p.isLocal());
        line("isMember", () -> "" + p.isMember());
        line("isTopLevel", () -> "" + p.isTopLevel());
        line("isVar", () -> "" + p.isVar());
        line("initializer", () -> el(p.getInitializer()));
        line("delegate", () -> el(p.getDelegate()));
        line("delegateExpression", () -> el(p.getDelegateExpression()));
        line("accessors", () -> els(p.getAccessors()));
        line("getter", () -> el(p.getGetter()));
        line("setter", () -> el(p.getSetter()));
      }
      if (e instanceof KtPropertyAccessor pa) {
        line("isGetter", () -> "" + pa.isGetter());
        line("isSetter", () -> "" + pa.isSetter());
        line("typeReference", () -> el(pa.getTypeReference()));
        line("valueParameterList", () -> el(pa.getParameterList()));
        line("valueParameters", () -> els(pa.getValueParameters()));
        line("initializer", () -> el(pa.getInitializer()));
      }
      if (e instanceof KtBackingField bf) {
        line("typeReference", () -> el(bf.getTypeReference()));
        line("initializer", () -> el(bf.getInitializer()));
      }
      if (e instanceof KtIsExpression is) line("typeReference", () -> el(is.getTypeReference()));
      if (e instanceof KtTypeProjection p) line("typeReference", () -> el(p.getTypeReference()));
      if (e instanceof KtConstructorCalleeExpression c) line("typeReference", () -> el(c.getTypeReference()));
      if (e instanceof KtFunctionTypeReceiver r) line("typeReference", () -> el(r.getTypeReference()));
      if (e instanceof KtParameter p) {
        line("hasDefaultValue", () -> "" + p.hasDefaultValue());
        line("defaultValue", () -> el(p.getDefaultValue()));
        line("isVarArg", () -> "" + p.isVarArg());
        line("isMutable", () -> "" + p.isMutable());
        line("hasValOrVar", () -> "" + p.hasValOrVar());
        line("valOrVarKeyword", () -> el(p.getValOrVarKeyword()));
        line("destructuringDeclaration", () -> el(p.getDestructuringDeclaration()));
        line("isLoopParameter", () -> "" + p.isLoopParameter());
        line("isCatchParameter", () -> "" + p.isCatchParameter());
        line("isContextParameter", () -> "" + p.isContextParameter());
        line("isLambdaParameter", () -> "" + p.isLambdaParameter());
        line("isFunctionTypeParameter", () -> "" + p.isFunctionTypeParameter());
      }
      if (e instanceof KtTypeAlias ta) {
        line("typeReference", () -> el(ta.getTypeReference()));
        line("isTopLevel", () -> "" + ta.isTopLevel());
      }
      if (e instanceof KtTypeParameter tp) line("extendsBound", () -> el(tp.getExtendsBound()));
      if (e instanceof KtTypeConstraint tc) {
        line("subjectTypeParameterName", () -> el(tc.getSubjectTypeParameterName()));
        line("boundTypeReference", () -> el(tc.getBoundTypeReference()));
      }
      if (e instanceof KtDestructuringDeclaration dd) {
        line("entries", () -> els(dd.getEntries()));
        line("initializer", () -> el(dd.getInitializer()));
        line("isVar", () -> "" + dd.isVar());
        line("valOrVarKeyword", () -> el(dd.getValOrVarKeyword()));
      }
      if (e instanceof KtDestructuringDeclarationEntry de) {
        line("isVar", () -> "" + de.isVar());
        line("valOrVarKeyword", () -> el(de.getValOrVarKeyword()));
        line("initializer", () -> el(de.getInitializer()));
      }
      if (e instanceof KtTypeReference tr) {
        line("typeElement", () -> el(tr.getTypeElement()));
        line("hasParentheses", () -> "" + tr.hasParentheses());
      }
      if (e instanceof KtTypeElement te) line("unwrapNullability", () -> el(KtPsiUtilKt.unwrapNullability(te)));
      if (e instanceof KtNullableType nt) {
        line("innerType", () -> el(nt.getInnerType()));
        line("modifierList", () -> el(nt.getModifierList()));
        line("annotationEntries", () -> els(nt.getAnnotationEntries()));
      }
      if (e instanceof KtUserType ut) {
        line("referenceExpression", () -> el(ut.getReferenceExpression()));
        line("qualifier", () -> el(ut.getQualifier()));
        line("referencedName", () -> str(ut.getReferencedName()));
        line("typeArgumentList", () -> el(ut.getTypeArgumentList()));
        line("typeArguments", () -> els(ut.getTypeArguments()));
      }
      if (e instanceof KtFunctionType ft) {
        line("receiver", () -> el(ft.getReceiver()));
        line("receiverTypeReference", () -> el(ft.getReceiverTypeReference()));
        line("parameterList", () -> el(ft.getParameterList()));
        line("parameters", () -> els(ft.getParameters()));
        line("returnTypeReference", () -> el(ft.getReturnTypeReference()));
      }
      if (e instanceof KtSimpleNameExpression s) {
        line("referencedName", () -> str(s.getReferencedName()));
        line("referencedNameElement", () -> el(s.getReferencedNameElement()));
      }
      if (e instanceof KtImportDirective id) {
        line("importedReference", () -> el(id.getImportedReference()));
        line("alias", () -> el(id.getAlias()));
        line("aliasName", () -> str(id.getAliasName()));
        line("isAllUnder", () -> "" + id.isAllUnder());
        line("importedFqName", () -> {
          FqName fq = id.getImportedFqName();
          return fq == null ? "null" : str(fq.asString());
        });
      }
      if (e instanceof KtQualifiedExpression q) {
        line("receiverExpression", () -> el(q.getReceiverExpressionOrNull()));
        line("selectorExpression", () -> el(q.getSelectorExpression()));
      }
      if (e instanceof KtCallElement c) {
        line("calleeExpression", () -> el(c.getCalleeExpression()));
        line("valueArgumentList", () -> el(c.getValueArgumentList()));
        line("valueArguments", () -> {
          List<PsiElement> args = new ArrayList<>();
          for (ValueArgument a : c.getValueArguments()) args.add(a.asElement());
          return els(args);
        });
        line("lambdaArguments", () -> els(c.getLambdaArguments()));
        line("typeArgumentList", () -> el(c.getTypeArgumentList()));
      }
      if (e instanceof KtValueArgument va) {
        line("argumentExpression", () -> el(va.getArgumentExpression()));
        line("argumentName", () -> el(va.getArgumentName()));
        line("isNamed", () -> "" + va.isNamed());
        line("isSpread", () -> "" + va.isSpread());
      }
      if (e instanceof KtLambdaArgument la) line("lambdaExpression", () -> el(la.getLambdaExpression()));
      if (e instanceof KtLambdaExpression le) {
        line("functionLiteral", () -> el(le.getFunctionLiteral()));
        line("valueParameters", () -> els(le.getValueParameters()));
      }
    }
  }
}
