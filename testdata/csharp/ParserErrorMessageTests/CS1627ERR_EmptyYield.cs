
using System.Collections;
class C : IEnumerable
{
   public IEnumerator GetEnumerator()
   {
      yield return;  // CS1627
   }
}
class Test
{
    public static int Main()
    {
        return 1;
    }
}
