using System;
public class TestAttribute : Attribute
{
  public TestAttribute(int i = 0, int j = 1) { }
  public int i { get; set; }
}
[Test(123, j:-1)]
public class A 
{
  public static int Main()
  {
    return 0;
  }
}
