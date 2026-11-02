
public delegate void D();
interface Itest
{
   event D E;
}
class Test : Itest
{
   event D ITest.E()   // CS0071
   {
   }
   public static int Main()
   {
       return 1;
   }
}
