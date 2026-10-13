var policy = document.getElementById("policy");
policy.textContent = "2N5 SCRIPT GREEN";
fetch("/2n5-data.txt").then(function(response) { return response.text(); }).then(function(text) { policy.textContent = policy.textContent + " " + text; });
