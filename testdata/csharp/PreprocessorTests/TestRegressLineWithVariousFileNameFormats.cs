public class LineTests
{
	public static void Main() 
	{
		#line 10 "ftp://test.cs"
		#line 20 "ftps://test.cs"
		#line 30 "http://test.cs"
		#line 40 "https://test.cs"
		#line 50 "gopher://test.cs"
		#line 60 "telnet://test.cs"
		#line 70 "dict://test.cs"
		#line 80 "file://goo.aspx"
		#line 90 "ldap://test.cs"
		#line 100 "news://test.cs"
		#line 110 "\\ddrelqa\logs\whidbey\2003-07-01\BVT64002\fx.Xml.XSLT\TESTPROCESSED20030701082505866.xml" // parser error
		#line 120 "C:\Documents and Settings\someuser\Local Settings\Temp\{f0a37341-d692-11d4-a984-009027ec0a9c}\test.cs" // parser error
		#line 130 "mailto://someuser@microsoft.com"
	}
}
