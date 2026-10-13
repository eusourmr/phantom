var markerNode = document.getElementById("defer-marker");
var statusNode = document.getElementById("status");
markerNode.textContent = "Deferred legacy MIME loaded";
fetch("/2n1-data.txt")
    .then(function (response) { return response.text(); })
    .then(function (text) {
        statusNode.className = "ready";
        statusNode.textContent = "2N1 REAL WEB GREEN " + text;
    });
