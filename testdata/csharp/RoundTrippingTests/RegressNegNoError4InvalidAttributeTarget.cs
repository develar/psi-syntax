using System;
using System.Reflection;

public class goo 
{
    [method: method:A]
    public static void Main() 
    {
    }
}
[AttributeUsage(AttributeTargets.All,AllowMultiple=true)]
public class A : Attribute
{
}